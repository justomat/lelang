use anyhow::{Context, Result};
use duckdb::Connection;
use std::path::Path;
use tracing::info;

use crate::api::{CatalogItem, LotDetail};

/// Initialize the DuckDB database and create tables if they don't exist.
pub fn init_db(db_path: &str) -> Result<Connection> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(db_path).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {db_path}"))?;
    }

    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open DuckDB at {db_path}"))?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS catalog_lots (
            id                    VARCHAR PRIMARY KEY,
            lot_lelang_id         VARCHAR UNIQUE,
            nama_lot_lelang       VARCHAR,
            nilai_limit           BIGINT,
            uang_jaminan          BIGINT,
            tanggal_batas_jaminan VARCHAR,
            nama_lokasi           VARCHAR,
            unit_kerja_id         VARCHAR,
            nama_unit_kerja       VARCHAR,
            tgl_mulai_lelang      VARCHAR,
            tgl_selesai_lelang    VARCHAR,
            status                VARCHAR,
            cara_penawaran        VARCHAR,
            version               INTEGER,
            scraped_at            TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS lot_details (
            id                    VARCHAR PRIMARY KEY,
            lot_lelang_id         VARCHAR,
            kode_lot              VARCHAR,
            views                 INTEGER,
            seller_nama           VARCHAR,
            seller_organisasi     VARCHAR,
            seller_telepon        VARCHAR,
            seller_alamat         VARCHAR,
            seller_kota           VARCHAR,
            seller_provinsi       VARCHAR,
            organizer_unit_kerja  VARCHAR,
            organizer_bank        VARCHAR,
            organizer_telepon     VARCHAR,
            organizer_alamat      VARCHAR,
            barangs_json          VARCHAR,
            latitude              DOUBLE,
            longitude             DOUBLE,
            scraped_at            TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS geocode_stats (
            month VARCHAR PRIMARY KEY,
            count INTEGER
        );
        ",
    )
    .context("Failed to create tables")?;

    // Add columns if they don't exist (for backward compatibility)
    let _ = conn.execute("ALTER TABLE lot_details ADD COLUMN IF NOT EXISTS latitude DOUBLE", []);
    let _ = conn.execute("ALTER TABLE lot_details ADD COLUMN IF NOT EXISTS longitude DOUBLE", []);

    info!("Database initialized at {db_path}");
    Ok(conn)
}

/// Parse a numeric string like "644360000" into an i64, returning 0 on failure.
fn parse_money(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or(0)
}

/// Upsert catalog items into the catalog_lots table.
pub fn upsert_catalog_items(conn: &Connection, items: &[CatalogItem]) -> Result<usize> {
    let mut stmt = conn.prepare(
        "INSERT INTO catalog_lots (
            id, lot_lelang_id, nama_lot_lelang, nilai_limit, uang_jaminan,
            tanggal_batas_jaminan, nama_lokasi, unit_kerja_id, nama_unit_kerja,
            tgl_mulai_lelang, tgl_selesai_lelang, status, cara_penawaran, version,
            scraped_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, CURRENT_TIMESTAMP)
        ON CONFLICT (id) DO UPDATE SET
            lot_lelang_id = excluded.lot_lelang_id,
            nama_lot_lelang = excluded.nama_lot_lelang,
            nilai_limit = excluded.nilai_limit,
            uang_jaminan = excluded.uang_jaminan,
            tanggal_batas_jaminan = excluded.tanggal_batas_jaminan,
            nama_lokasi = excluded.nama_lokasi,
            unit_kerja_id = excluded.unit_kerja_id,
            nama_unit_kerja = excluded.nama_unit_kerja,
            tgl_mulai_lelang = excluded.tgl_mulai_lelang,
            tgl_selesai_lelang = excluded.tgl_selesai_lelang,
            status = excluded.status,
            cara_penawaran = excluded.cara_penawaran,
            version = excluded.version,
            scraped_at = now()",
    )?;

    let mut count = 0usize;
    for item in items {
        stmt.execute(duckdb::params![
            item.id,
            item.lot_lelang_id,
            item.nama_lot_lelang,
            parse_money(&item.nilai_limit),
            parse_money(&item.uang_jaminan),
            item.tanggal_batas_jaminan.as_deref().unwrap_or(""),
            item.nama_lokasi.as_deref().unwrap_or(""),
            item.unit_kerja_id.as_deref().unwrap_or(""),
            item.nama_unit_kerja.as_deref().unwrap_or(""),
            item.tgl_mulai_lelang.as_deref().unwrap_or(""),
            item.tgl_selesai_lelang.as_deref().unwrap_or(""),
            item.status.as_deref().unwrap_or(""),
            item.cara_penawaran.as_deref().unwrap_or(""),
            item.version.unwrap_or(0) as i32,
        ])?;
        count += 1;
    }

    info!("Upserted {count} catalog items");
    Ok(count)
}

/// Upsert a lot detail into the lot_details table.
pub fn upsert_lot_detail(conn: &Connection, detail: &LotDetail, lat: Option<f64>, lng: Option<f64>) -> Result<()> {
    let (seller_nama, seller_org, seller_tel, seller_addr, seller_kota, seller_prov) =
        if let Some(ref content) = detail.content {
            if let Some(ref s) = content.seller {
                (
                    s.nama_penjual.as_deref().unwrap_or(""),
                    s.nama_organisasi_penjual.as_deref().unwrap_or(""),
                    s.nomor_telepon.as_deref().unwrap_or(""),
                    s.alamat.as_deref().unwrap_or(""),
                    s.nama_kota.as_deref().unwrap_or(""),
                    s.nama_provinsi.as_deref().unwrap_or(""),
                )
            } else {
                ("", "", "", "", "", "")
            }
        } else {
            ("", "", "", "", "", "")
        };

    let (org_uk, org_bank, org_tel, org_addr) = if let Some(ref content) = detail.content {
        if let Some(ref o) = content.organizer {
            (
                o.nama_unit_kerja.as_deref().unwrap_or(""),
                o.nama_bank.as_deref().unwrap_or(""),
                o.nomor_telepon.as_deref().unwrap_or(""),
                o.alamat.as_deref().unwrap_or(""),
            )
        } else {
            ("", "", "", "")
        }
    } else {
        ("", "", "", "")
    };

    let barangs_json = if let Some(ref content) = detail.content {
        serde_json::to_string(&content.barangs).unwrap_or_else(|_| "[]".to_string())
    } else {
        "[]".to_string()
    };

    conn.execute(
        "INSERT INTO lot_details (
            id, lot_lelang_id, kode_lot, views,
            seller_nama, seller_organisasi, seller_telepon, seller_alamat,
            seller_kota, seller_provinsi,
            organizer_unit_kerja, organizer_bank, organizer_telepon, organizer_alamat,
            barangs_json, latitude, longitude, scraped_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, now())
        ON CONFLICT (id) DO UPDATE SET
            lot_lelang_id = excluded.lot_lelang_id,
            kode_lot = excluded.kode_lot,
            views = excluded.views,
            seller_nama = excluded.seller_nama,
            seller_organisasi = excluded.seller_organisasi,
            seller_telepon = excluded.seller_telepon,
            seller_alamat = excluded.seller_alamat,
            seller_kota = excluded.seller_kota,
            seller_provinsi = excluded.seller_provinsi,
            organizer_unit_kerja = excluded.organizer_unit_kerja,
            organizer_bank = excluded.organizer_bank,
            organizer_telepon = excluded.organizer_telepon,
            organizer_alamat = excluded.organizer_alamat,
            barangs_json = excluded.barangs_json,
            latitude = excluded.latitude,
            longitude = excluded.longitude,
            scraped_at = now()",
        duckdb::params![
            detail.id,
            detail.lot_lelang_id,
            detail.kode_lot.as_deref().unwrap_or(""),
            detail.views.unwrap_or(0) as i32,
            seller_nama,
            seller_org,
            seller_tel,
            seller_addr,
            seller_kota,
            seller_prov,
            org_uk,
            org_bank,
            org_tel,
            org_addr,
            barangs_json,
            lat,
            lng,
        ],
    )?;

    Ok(())
}

/// Get all lot_lelang_ids from catalog that don't have detail records yet.
pub fn get_unscraped_lot_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT c.lot_lelang_id
         FROM catalog_lots c
         LEFT JOIN lot_details d ON c.lot_lelang_id = d.lot_lelang_id
         WHERE d.id IS NULL
         ORDER BY c.scraped_at DESC",
    )?;

    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;

    Ok(ids)
}

/// Get ALL lot_lelang_ids from catalog.
pub fn get_all_lot_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT lot_lelang_id FROM catalog_lots ORDER BY scraped_at DESC",
    )?;

    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;

    Ok(ids)
}

/// Export tables to Parquet files.
pub fn export_parquet(conn: &Connection, output_dir: &str) -> Result<()> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("Failed to create output dir {output_dir}"))?;

    let catalog_path = format!("{output_dir}/catalog_lots.parquet");
    let details_path = format!("{output_dir}/lot_details.parquet");

    conn.execute(
        &format!("COPY catalog_lots TO '{catalog_path}' (FORMAT PARQUET)"),
        [],
    )
    .with_context(|| format!("Failed to export catalog_lots to {catalog_path}"))?;
    info!("Exported catalog_lots → {catalog_path}");

    // Only export lot_details if the table has rows
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM lot_details", [], |row| row.get(0))?;

    if count > 0 {
        conn.execute(
            &format!("COPY lot_details TO '{details_path}' (FORMAT PARQUET)"),
            [],
        )
        .with_context(|| format!("Failed to export lot_details to {details_path}"))?;
        info!("Exported lot_details ({count} rows) → {details_path}");
    } else {
        info!("Skipping lot_details export (no rows)");
    }

    Ok(())
}

/// Print summary stats about the database.
pub fn print_stats(conn: &Connection) -> Result<()> {
    let catalog_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM catalog_lots", [], |row| row.get(0))?;
    let detail_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM lot_details", [], |row| row.get(0))?;

    println!("  catalog_lots:  {catalog_count} rows");
    println!("  lot_details:   {detail_count} rows");

    if catalog_count > 0 {
        let statuses: Vec<(String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT status, COUNT(*) as cnt FROM catalog_lots GROUP BY status ORDER BY cnt DESC",
            )?;
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        println!("  Status breakdown:");
        for (status, cnt) in statuses {
            println!("    {status}: {cnt}");
        }
    }

    Ok(())
}
