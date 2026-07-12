# Server-Side Geocoding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Google Maps geocoding from the frontend browser to the Rust scraper pipeline to directly save coordinates into the DuckDB database and export them via Parquet.

**Architecture:** We will introduce a new `src/geocoding.rs` module responsible for interacting with the Google Maps Geocoding API and managing the monthly rate limit using a new DuckDB table. The detail-scraping command in `src/main.rs` will orchestrate calling this geocoder right before saving the `LotDetail` to the database, ensuring we extract the address correctly. Finally, we'll strip the geocoding logic out of the frontend.

**Tech Stack:** Rust, reqwest, DuckDB, Parquet, Vanilla JS, HTML.

---

### Task 1: Add New Dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add dependencies to Cargo.toml**
Update `Cargo.toml` to include `dotenvy` for environment variable loading and `chrono` for month tracking.

```toml
[dependencies]
# HTTP client — rustls-tls to bypass TLS fingerprint blocking (LibreSSL is blocked)
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio = { version = "1", features = ["full", "macros"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Database + Parquet export
duckdb = { version = "1.1", features = ["bundled"] }

# CLI
clap = { version = "4", features = ["derive"] }

# UX
indicatif = "0.17"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Error handling
anyhow = "1"
primp = "1.3.1"

# Environment & Time
dotenvy = "0.15"
chrono = "0.4"
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add Cargo.toml Cargo.lock
git commit -m "chore: add dotenvy and chrono dependencies"
```

### Task 2: Database Schema Updates

**Files:**
- Modify: `src/db.rs`

- [ ] **Step 1: Update `init_db` for new columns and tables**
Modify `init_db` to include `latitude` and `longitude` in the `lot_details` table creation, add `ALTER TABLE` statements for existing databases, and create the `geocode_stats` table.

```rust
pub fn init_db(db_path: &str) -> Result<Connection> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(db_path).parent() {
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

    tracing::info!("Database initialized at {db_path}");
    Ok(conn)
}
```

- [ ] **Step 2: Update `upsert_lot_detail` signature and query**
Change the signature of `upsert_lot_detail` to accept `latitude` and `longitude` and update the SQL query.

```rust
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
```

- [ ] **Step 3: Commit**

```bash
cargo check
git add src/db.rs
git commit -m "feat: add latitude and longitude to lot_details table"
```

### Task 3: Geocoding Module

**Files:**
- Create: `src/geocoding.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Declare the new module**
Add `mod geocoding;` to the top of `src/main.rs`.

```rust
mod api;
mod db;
mod geocoding;
```

- [ ] **Step 2: Create `src/geocoding.rs`**
Implement the API structures, fetching, and rate limiting logic.

```rust
use anyhow::{Context, Result};
use duckdb::Connection;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GeocodeResponse {
    pub results: Vec<GeocodeResult>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct GeocodeResult {
    pub geometry: Geometry,
}

#[derive(Debug, Deserialize)]
pub struct Geometry {
    pub location: Location,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lng: f64,
}

/// Call Google Maps Geocoding API.
pub async fn geocode(client: &Client, api_key: &str, address: &str) -> Result<Option<Location>> {
    let url = format!(
        "https://maps.googleapis.com/maps/api/geocode/json?address={}&key={}",
        urlencoding::encode(address),
        api_key
    );

    let resp: GeocodeResponse = client
        .get(&url)
        .send()
        .await
        .context("Geocoding request failed")?
        .json()
        .await
        .context("Failed to parse geocoding response")?;

    if resp.status == "OK" {
        if let Some(first_result) = resp.results.into_iter().next() {
            return Ok(Some(first_result.geometry.location));
        }
    }
    
    Ok(None)
}

/// Get the current month's geocoding count.
pub fn get_geocode_count(conn: &Connection, month: &str) -> Result<u32> {
    let count: u32 = conn.query_row(
        "SELECT count FROM geocode_stats WHERE month = ?",
        duckdb::params![month],
        |row| row.get(0),
    ).unwrap_or(0);
    Ok(count)
}

/// Increment the current month's geocoding count.
pub fn increment_geocode_count(conn: &Connection, month: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO geocode_stats (month, count) VALUES (?, 1)
         ON CONFLICT (month) DO UPDATE SET count = count + 1",
        duckdb::params![month],
    )?;
    Ok(())
}
```

- [ ] **Step 3: Commit**

```bash
cargo check
git add src/main.rs src/geocoding.rs
git commit -m "feat: implement Google Maps Geocoding integration with DuckDB quota tracking"
```

### Task 4: Hook Up Geocoding in Scraper Pipeline

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Extract API key and do geocoding inside `cmd_detail`**
Update `cmd_detail` to load `.env`, fetch the API key, extract the address, check quota limits, geocode if applicable, and pass the coordinates to `db::upsert_lot_detail`.

```rust
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
```

- [ ] **Step 2: Commit**

```bash
cargo check
git add src/main.rs
git commit -m "feat: hook up geocoding pipeline in cmd_detail"
```

### Task 5: Frontend Cleanup

**Files:**
- Modify: `index.html`
- Modify: `app.js`

- [ ] **Step 1: Clean up `index.html`**
Remove the geocoding stat and progress bar containers since these are now handled on the backend. Leave the general initialized stats and API Key modal (which we still need to load the Maps UI).

Replace lines 26-36:
```html
            <div class="stat-item">
                <span class="stat-label">Geocoded (This Month):</span>
                <span id="stat-geocoded">0 / 1000</span>
            </div>
        </div>
        
        <div class="progress-container" id="geocoding-progress-container" style="display: none;">
            <p class="stat-label">Geocoding Queue...</p>
            <div class="progress-bar-bg">
                <div id="geocoding-progress-fill" class="progress-bar-fill"></div>
            </div>
        </div>
```
With:
```html
        </div>
```

- [ ] **Step 2: Clean up `app.js` geocoding logic and update SQL query**
Update the DB query to extract `d.latitude` and `d.longitude`. Remove the entire block for hitting the Google Maps Geocoder, local quotas, and tracking.

```javascript
import * as duckdb from 'https://cdn.jsdelivr.net/npm/@duckdb/duckdb-wasm@1.28.1-dev106.0/+esm';

let map;
const markers = [];

async function getApiKey() {
    try {
        const res = await fetch('.env');
        if (res.ok) {
            const text = await res.text();
            const match = text.match(/GOOGLE_MAPS_API_KEY=(.+)/);
            if (match && match[1]) {
                return match[1].trim();
            }
        }
    } catch (e) {
        console.warn("Could not fetch .env file, checking localStorage");
    }

    const cachedKey = localStorage.getItem('gmaps_api_key');
    if (cachedKey) return cachedKey;

    return new Promise((resolve) => {
        const modal = document.getElementById('api-key-modal');
        const input = document.getElementById('api-key-input');
        const btn = document.getElementById('save-api-key-btn');
        
        modal.style.display = 'flex';
        
        btn.onclick = () => {
            const val = input.value.trim();
            if (val) {
                localStorage.setItem('gmaps_api_key', val);
                modal.style.display = 'none';
                resolve(val);
            }
        };
    });
}

function loadGoogleMaps(apiKey) {
    return new Promise((resolve, reject) => {
        if (window.google && window.google.maps) {
            resolve();
            return;
        }
        window.initMap = () => {
            resolve();
        };
        const script = document.createElement('script');
        script.src = `https://maps.googleapis.com/maps/api/js?key=${apiKey}&callback=initMap`;
        script.async = true;
        script.defer = true;
        script.onerror = reject;
        document.head.appendChild(script);
    });
}

async function init() {
    const statusText = document.getElementById('status-text');
    const statusIndicator = document.getElementById('status-indicator');

    function setStatus(msg, type) {
        statusText.textContent = msg;
        statusIndicator.className = `pulse ${type}`;
    }

    try {
        // 1. Setup API & Map
        setStatus("Waiting for API Key...", "loading");
        const apiKey = await getApiKey();
        
        setStatus("Loading Google Maps...", "loading");
        await loadGoogleMaps(apiKey);
        
        map = new google.maps.Map(document.getElementById("map"), {
            center: { lat: -2.5489, lng: 118.0149 }, // Center of Indonesia
            zoom: 5,
            mapId: "DEMO_MAP_ID",
            disableDefaultUI: true,
            zoomControl: true,
        });

        // 2. Load DuckDB & Data
        setStatus("Initializing DuckDB Engine...", "loading");
        
        const fetchCatPromise = fetch('data/catalog_lots.parquet').then(res => {
            if (!res.ok) throw new Error("Failed to fetch catalog_lots.parquet");
            return res.arrayBuffer();
        });
        const fetchDetPromise = fetch('data/lot_details.parquet').then(res => {
            if (!res.ok) throw new Error("Failed to fetch lot_details.parquet");
            return res.arrayBuffer();
        });

        const JSDELIVR_BUNDLES = duckdb.getJsDelivrBundles();
        const bundle = await duckdb.selectBundle(JSDELIVR_BUNDLES);
        const worker_url = URL.createObjectURL(
            new Blob([`importScripts("${bundle.mainWorker}");`], { type: 'text/javascript' })
        );
        const worker = new Worker(worker_url);
        const logger = new duckdb.ConsoleLogger();
        const db = new duckdb.AsyncDuckDB(logger, worker);
        await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
        URL.revokeObjectURL(worker_url);

        const [bufCat, bufDet] = await Promise.all([fetchCatPromise, fetchDetPromise]);

        await db.registerFileBuffer('catalog.parquet', new Uint8Array(bufCat));
        await db.registerFileBuffer('details.parquet', new Uint8Array(bufDet));

        const conn = await db.connect();
        
        setStatus("Extracting Lot Locations...", "loading");

        // 3. Query Lots with pre-computed coordinates
        const query = await conn.query(`
            SELECT 
                c.lot_lelang_id, 
                c.nama_lot_lelang, 
                c.nilai_limit, 
                c.status,
                COALESCE(d.barangs_json->0->>'alamat', c.nama_lokasi) as address,
                d.latitude as lat,
                d.longitude as lng
            FROM 'catalog.parquet' c
            LEFT JOIN 'details.parquet' d ON c.lot_lelang_id = d.lot_lelang_id
            WHERE d.latitude IS NOT NULL AND d.longitude IS NOT NULL
            LIMIT 500
        `);
        const rows = query.toArray();
        
        document.getElementById('stat-loaded').textContent = rows.length.toString();
        
        // 4. Plot Markers
        setStatus(`Plotting ${rows.length} lots...`, "loading");

        for (const row of rows) {
            const marker = new google.maps.Marker({
                position: { lat: row.lat, lng: row.lng },
                map: map,
                title: row.nama_lot_lelang
            });
            
            const infoWindow = new google.maps.InfoWindow({
                content: `
                    <div class="info-window">
                        <span class="status">${row.status || 'UNKNOWN'}</span>
                        <h3>${row.nama_lot_lelang}</h3>
                        <p>${row.address || 'No Address'}</p>
                        <p class="price">Rp ${Number(row.nilai_limit).toLocaleString()}</p>
                    </div>
                `
            });

            marker.addListener("click", () => {
                infoWindow.open({
                    anchor: marker,
                    map,
                });
            });
            
            markers.push(marker);
        }
        
        setStatus("Map Ready", "success");

    } catch (e) {
        console.error(e);
        setStatus("Error: " + e.message, "error");
    }
}

init();
```

- [ ] **Step 3: Commit**

```bash
git add index.html app.js
git commit -m "refactor: remove geocoding logic from frontend in favor of DB coordinates"
```
