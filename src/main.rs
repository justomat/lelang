mod api;
mod db;
mod geocoding;

use anyhow::Result;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use tracing::info;

#[derive(Parser)]
#[command(name = "lelang-scraper")]
#[command(about = "Scrape auction data from lelang.go.id into DuckDB + Parquet")]
struct Cli {
    /// DuckDB file path
    #[arg(long, default_value = "data/lelang.db")]
    db: String,

    /// Items per page for catalog scraping
    #[arg(long, default_value_t = 50)]
    page_size: u32,

    /// Delay in milliseconds between API requests
    #[arg(long, default_value_t = 200)]
    delay_ms: u64,

    /// Max pages to fetch (0 = all)
    #[arg(long, default_value_t = 0)]
    max_pages: u32,

    /// Comma-separated categories
    #[arg(long, default_value = "Ruko,Rumah,Tanah", value_delimiter = ',')]
    categories: Vec<String>,

    /// Comma-separated province names or UUIDs (empty = all)
    #[arg(long, value_delimiter = ',')]
    provinces: Vec<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all 39 provinces with UUID and name
    Provinces,
    /// Scrape the lot catalog (paginated, filtered by category/province)
    Catalog,
    /// Scrape detailed info for catalog lots not yet in lot_details
    Detail {
        /// Re-scrape all lots, not just missing ones
        #[arg(long, default_value_t = false)]
        all: bool,
    },
    /// Export DuckDB tables to Parquet files
    Export,
    /// Run full pipeline: catalog → detail → export
    Full {
        /// Re-scrape all lot details, not just missing ones
        #[arg(long, default_value_t = false)]
        all: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let client = api::ApiClient::new(cli.delay_ms)?;

    match cli.command {
        Commands::Provinces => {
            cmd_provinces(&client).await?;
        }
        Commands::Catalog => {
            let conn = db::init_db(&cli.db)?;
            cmd_catalog(&client, &conn, &cli).await?;
            db::print_stats(&conn)?;
        }
        Commands::Detail { all } => {
            let conn = db::init_db(&cli.db)?;
            cmd_detail(&client, &conn, all).await?;
            db::print_stats(&conn)?;
        }
        Commands::Export => {
            let conn = db::init_db(&cli.db)?;
            let output_dir = std::path::Path::new(&cli.db)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_str()
                .unwrap_or("data");
            db::export_parquet(&conn, output_dir)?;
        }
        Commands::Full { all } => {
            let conn = db::init_db(&cli.db)?;
            cmd_catalog(&client, &conn, &cli).await?;
            cmd_detail(&client, &conn, all).await?;

            let output_dir = std::path::Path::new(&cli.db)
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_str()
                .unwrap_or("data");
            db::export_parquet(&conn, output_dir)?;

            println!("\n📊 Final stats:");
            db::print_stats(&conn)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_provinces(client: &api::ApiClient) -> Result<()> {
    let provinces = client.fetch_provinces().await?;

    println!("\n{:<40} {:<6} {}", "UUID", "CODE", "NAME");
    println!("{}", "-".repeat(80));
    for p in &provinces {
        println!(
            "{:<40} {:<6} {}",
            p.id,
            p.code.as_deref().unwrap_or(""),
            p.nama
        );
    }
    println!("\nTotal: {} provinces", provinces.len());
    println!("\nUsage: --provinces \"DKI JAKARTA,JAWA BARAT\"");

    Ok(())
}

async fn cmd_catalog(
    client: &api::ApiClient,
    conn: &duckdb::Connection,
    cli: &Cli,
) -> Result<()> {
    // Resolve province names → UUIDs
    let province_ids = client.resolve_province_ids(&cli.provinces).await?;

    println!(
        "\n🔍 Scraping catalog (categories: {:?}, provinces: {})",
        cli.categories,
        if province_ids.is_empty() {
            "all".to_string()
        } else {
            format!("{} selected", province_ids.len())
        }
    );

    let items = client
        .fetch_all_catalog(cli.page_size, cli.max_pages, &cli.categories, &province_ids)
        .await?;

    if items.is_empty() {
        println!("⚠️  No items found with the given filters.");
        return Ok(());
    }

    let count = db::upsert_catalog_items(conn, &items)?;
    println!("✅ Saved {count} catalog items to DB\n");

    Ok(())
}

async fn cmd_detail(
    client: &api::ApiClient,
    conn: &duckdb::Connection,
    scrape_all: bool,
) -> Result<()> {
    // Load .env silently
    let _ = dotenvy::dotenv();
    let api_key = std::env::var("GOOGLE_MAPS_API_KEY").ok();
    if api_key.is_none() {
        tracing::warn!("GOOGLE_MAPS_API_KEY not set. Geocoding will be skipped.");
    }

    let current_month = chrono::Utc::now().format("%Y-%m").to_string();
    let geocode_client = reqwest::Client::new();

    let lot_ids = if scrape_all {
        db::get_all_lot_ids(conn)?
    } else {
        db::get_unscraped_lot_ids(conn)?
    };

    if lot_ids.is_empty() {
        println!("✅ All lot details are up to date (nothing to scrape).");
        return Ok(());
    }

    println!("\n🔍 Fetching details for {} lots...", lot_ids.len());

    let pb = ProgressBar::new(lot_ids.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )
        .unwrap()
        .progress_chars("█▓░"),
    );

    let results = client.fetch_lot_details(&lot_ids, &pb).await;
    pb.finish_with_message("done");

    let mut success = 0usize;
    let mut failed = 0usize;
    let mut geocoded_count = 0usize;

    for (id, result) in &results {
        match result {
            Ok(detail) => {
                let mut lat = None;
                let mut lng = None;

                // Attempt to geocode if we have an API key and haven't exceeded the 1000/month limit
                if let Some(key) = &api_key {
                    let address = detail.content
                        .as_ref()
                        .and_then(|c| c.barangs.as_ref())
                        .and_then(|b| b.first())
                        .and_then(|b| b.alamat.clone())
                        .or_else(|| detail.nama_lokasi.clone());

                    if let Some(addr) = address {
                        if !addr.trim().is_empty() {
                            let quota_used = geocoding::get_geocode_count(conn, &current_month).unwrap_or(0);
                            if quota_used < 1000 {
                                match geocoding::geocode(&geocode_client, key, &addr).await {
                                    Ok(Some(location)) => {
                                        lat = Some(location.lat);
                                        lng = Some(location.lng);
                                        let _ = geocoding::increment_geocode_count(conn, &current_month);
                                        geocoded_count += 1;
                                    }
                                    Ok(None) => {}
                                    Err(e) => tracing::warn!("Failed to geocode address '{}': {}", addr, e),
                                }
                                // Polite delay to respect API limits
                                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                            } else {
                                tracing::debug!("Geocoding quota (1000) reached for {}, skipping.", current_month);
                            }
                        }
                    }
                }

                if let Err(e) = db::upsert_lot_detail(conn, detail, lat, lng) {
                    eprintln!("  DB error for {id}: {e:#}");
                    failed += 1;
                } else {
                    success += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    println!("✅ Details: {success} saved, {failed} failed (Geocoded this run: {geocoded_count})\n");

    Ok(())
}
