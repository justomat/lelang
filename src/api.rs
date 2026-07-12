use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ORIGIN, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

const API_BASE: &str = "https://api.lelang.go.id/api/v1";
const API_AUTH_BASE: &str = "https://api-auth.lelang.go.id/api/v1";

// ---------------------------------------------------------------------------
// API response wrappers
// ---------------------------------------------------------------------------

/// Generic paginated response from api.lelang.go.id
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub code: u32,
    pub message: String,
    pub data: T,
    pub page: Option<u32>,
    #[serde(rename = "perPage")]
    pub per_page: Option<u32>,
    #[serde(rename = "totalPage")]
    pub total_page: Option<u32>,
    #[serde(rename = "totalItem")]
    pub total_item: Option<u32>,
}

/// Response from api-auth.lelang.go.id (uses snake_case)
#[derive(Debug, Deserialize)]
pub struct AuthApiResponse<T> {
    pub status: u32,
    pub message: String,
    pub data: T,
    pub total_item: Option<u32>,
}

// ---------------------------------------------------------------------------
// Province (from api-auth.lelang.go.id)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Province {
    pub id: String,
    pub nama: String,
    pub full_code: Option<String>,
    pub code: Option<String>,
}

// ---------------------------------------------------------------------------
// Catalog item (from katalog-lot-lelang)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub lot_lelang_id: String,
    pub nama_lot_lelang: String,
    pub nilai_limit: String,
    pub uang_jaminan: String,
    pub tanggal_batas_jaminan: Option<String>,
    pub nama_lokasi: Option<String>,
    pub unit_kerja_id: Option<String>,
    pub nama_unit_kerja: Option<String>,
    pub tgl_mulai_lelang: Option<String>,
    pub tgl_selesai_lelang: Option<String>,
    pub status: Option<String>,
    pub cara_penawaran: Option<String>,
    pub version: Option<u32>,
    // photos omitted — we store photo URLs separately if needed
}

// ---------------------------------------------------------------------------
// Lot detail (from /info/{id})
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LotDetail {
    pub id: String,
    pub permohonan_id: Option<String>,
    pub lot_lelang_id: String,
    pub nama_lot_lelang: String,
    pub nilai_limit: String,
    pub uang_jaminan: String,
    pub kode_lot: Option<String>,
    pub tanggal_batas_jaminan: Option<String>,
    pub nama_lokasi: Option<String>,
    pub unit_kerja_id: Option<String>,
    pub nama_unit_kerja: Option<String>,
    pub tgl_mulai_lelang: Option<String>,
    pub tgl_selesai_lelang: Option<String>,
    pub status: Option<String>,
    pub cara_penawaran: Option<String>,
    pub version: Option<u32>,
    pub content: Option<LotContent>,
    pub views: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LotContent {
    pub barangs: Option<Vec<Barang>>,
    pub seller: Option<Seller>,
    pub organizer: Option<Organizer>,
    pub attachments: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Barang {
    pub id: String,
    pub nama: Option<String>,
    pub bukti_kepemilikan: Option<String>,
    pub bukti_kepemilikan_no: Option<String>,
    pub alamat: Option<String>,
    pub luas: Option<String>,
    pub stnk: Option<String>,
    pub nomor_rangka: Option<String>,
    pub nopol: Option<String>,
    pub tahun: Option<String>,
    pub warna: Option<String>,
    pub jenis_barang: Option<JenisRef>,
    pub jenis_objek: Option<JenisRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JenisRef {
    pub id: String,
    pub nama: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seller {
    pub nama_penjual: Option<String>,
    pub nama_organisasi_penjual: Option<String>,
    pub nomor_telepon: Option<String>,
    pub alamat: Option<String>,
    pub nama_kota: Option<String>,
    pub nama_provinsi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organizer {
    pub nama_unit_kerja: Option<String>,
    pub nama_bank: Option<String>,
    pub nomor_telepon: Option<String>,
    pub alamat: Option<String>,
}

// ---------------------------------------------------------------------------
// HTTP Client
// ---------------------------------------------------------------------------

pub struct ApiClient {
    client: reqwest::Client,
    delay: Duration,
}

impl ApiClient {
    pub fn new(delay_ms: u64) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/150.0.0.0 Safari/537.36",
            ),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
        headers.insert(ORIGIN, HeaderValue::from_static("https://lelang.go.id"));
        headers.insert(REFERER, HeaderValue::from_static("https://lelang.go.id/"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .use_rustls_tls()
            .http1_only()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            delay: Duration::from_millis(delay_ms),
        })
    }

    /// Polite delay between requests
    async fn throttle(&self) {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
    }

    /// Execute a request with automatic retries for transient network errors.
    async fn execute_with_retry(&self, req_builder: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut attempts = 0;
        let max_attempts = 5;

        loop {
            // Clone the request builder for retries. 
            // reqwest::RequestBuilder implements TryClone for requests without streaming bodies.
            let req = req_builder.try_clone().context("Failed to clone request")?;
            
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                        attempts += 1;
                        if attempts >= max_attempts {
                            return resp.error_for_status().context("API returned error status after retries");
                        }
                        warn!("Server error {status}, retrying ({attempts}/{max_attempts})...");
                        tokio::time::sleep(Duration::from_secs(2 * attempts as u64)).await;
                        continue;
                    }
                    return resp.error_for_status().context("API returned error status");
                }
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(anyhow::anyhow!(e).context("Connection failed after retries"));
                    }
                    warn!("Network error: {e}, retrying ({attempts}/{max_attempts})...");
                    tokio::time::sleep(Duration::from_secs(2 * attempts as u64)).await;
                }
            }
        }
    }

    // ----- Provinces -----

    pub async fn fetch_provinces(&self) -> Result<Vec<Province>> {
        let url = format!("{API_AUTH_BASE}/master/ref-provinsi?limit=9999");
        info!("Fetching provinces from {url}");

        let req = self.client.get(&url);
        let resp: AuthApiResponse<Vec<Province>> = self
            .execute_with_retry(req)
            .await
            .context("Failed to fetch provinces")?
            .json()
            .await
            .context("Failed to parse provinces JSON")?;

        info!("Found {} provinces", resp.data.len());
        Ok(resp.data)
    }

    /// Resolve province names/UUIDs to UUIDs. Accepts either a UUID or a
    /// case-insensitive substring of the province name.
    pub async fn resolve_province_ids(&self, inputs: &[String]) -> Result<Vec<String>> {
        if inputs.is_empty() {
            return Ok(vec![]);
        }

        let provinces = self.fetch_provinces().await?;
        let mut resolved = Vec::new();

        for input in inputs {
            let trimmed = input.trim();
            // If it looks like a UUID, use as-is
            if trimmed.contains('-') && trimmed.len() > 30 {
                resolved.push(trimmed.to_string());
                continue;
            }
            // Otherwise match by name (case-insensitive substring)
            let needle = trimmed.to_uppercase();
            let found: Vec<_> = provinces
                .iter()
                .filter(|p| p.nama.to_uppercase().contains(&needle))
                .collect();

            match found.len() {
                0 => anyhow::bail!("No province matches '{trimmed}'. Use `provinces` command to list all."),
                1 => {
                    info!("Resolved '{}' → {} ({})", trimmed, found[0].nama, found[0].id);
                    resolved.push(found[0].id.clone());
                }
                _ => {
                    let names: Vec<_> = found.iter().map(|p| p.nama.as_str()).collect();
                    anyhow::bail!(
                        "'{trimmed}' matches multiple provinces: {}. Be more specific.",
                        names.join(", ")
                    );
                }
            }
        }

        Ok(resolved)
    }

    // ----- Catalog -----

    pub async fn fetch_catalog_page(
        &self,
        page: u32,
        page_size: u32,
        categories: &[String],
        province_id: Option<&str>,
    ) -> Result<PaginatedResponse<Vec<CatalogItem>>> {
        let mut url = format!(
            "{API_BASE}/landing-page/katalog-lot-lelang?limit={page_size}&dcp=true&page={page}"
        );

        for cat in categories {
            url.push_str(&format!("&namakategori%5B%5D={cat}"));
        }

        if let Some(prov) = province_id {
            url.push_str(&format!("&province={prov}"));
        }

        debug!("Fetching catalog page {page}: {url}");

        let req = self.client.get(&url);
        let resp: PaginatedResponse<Vec<CatalogItem>> = self
            .execute_with_retry(req)
            .await
            .with_context(|| format!("Failed to fetch catalog page {page}"))?
            .json()
            .await
            .with_context(|| format!("Failed to parse catalog page {page}"))?;

        Ok(resp)
    }

    /// Fetch all catalog pages, returning all items.
    pub async fn fetch_all_catalog(
        &self,
        page_size: u32,
        max_pages: u32,
        categories: &[String],
        province_ids: &[String],
    ) -> Result<Vec<CatalogItem>> {
        let mut all_items = Vec::new();

        // If no province filter, fetch once with no province param
        let prov_list: Vec<Option<&str>> = if province_ids.is_empty() {
            vec![None]
        } else {
            province_ids.iter().map(|s| Some(s.as_str())).collect()
        };

        for prov in &prov_list {
            let mut page = 1u32;
            let mut pb = None;

            loop {
                let resp = self.fetch_catalog_page(page, page_size, categories, *prov).await?;

                let total_pages = resp.total_page.unwrap_or(1);
                let total_items = resp.total_item.unwrap_or(0);
                let items_on_page = resp.data.len();

                if page == 1 {
                    info!(
                        "Catalog: {total_items} total items, {total_pages} pages (province: {})",
                        prov.unwrap_or("all")
                    );
                    
                    let new_pb = indicatif::ProgressBar::new(total_pages as u64);
                    new_pb.set_style(
                        indicatif::ProgressStyle::with_template(
                            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} pages ({eta})",
                        )
                        .unwrap()
                        .progress_chars("█▓░"),
                    );
                    pb = Some(new_pb);
                }

                all_items.extend(resp.data);
                
                if let Some(ref p) = pb {
                    p.inc(1);
                }

                if page >= total_pages || items_on_page == 0 {
                    break;
                }
                if max_pages > 0 && page >= max_pages {
                    info!("Reached max_pages limit ({max_pages})");
                    break;
                }

                page += 1;
                self.throttle().await;
            }
            
            if let Some(p) = pb {
                p.finish_with_message("done");
            }
        }

        info!("Fetched {} catalog items total", all_items.len());
        Ok(all_items)
    }

    // ----- Lot detail -----

    pub async fn fetch_lot_detail(&self, lot_lelang_id: &str) -> Result<LotDetail> {
        let url = format!("{API_BASE}/landing-page/info/{lot_lelang_id}");
        debug!("Fetching detail: {url}");

        let req = self.client.get(&url);
        let resp: PaginatedResponse<LotDetail> = self
            .execute_with_retry(req)
            .await
            .with_context(|| format!("Failed to fetch detail for {lot_lelang_id}"))?
            .json()
            .await
            .with_context(|| format!("Failed to parse detail for {lot_lelang_id}"))?;

        Ok(resp.data)
    }

    /// Fetch details for a batch of lot IDs with progress and throttling.
    pub async fn fetch_lot_details(
        &self,
        lot_ids: &[String],
        progress: &indicatif::ProgressBar,
    ) -> Vec<(String, Result<LotDetail>)> {
        let mut results = Vec::with_capacity(lot_ids.len());

        for id in lot_ids {
            let result = self.fetch_lot_detail(id).await;
            if let Err(ref e) = result {
                warn!("Failed to fetch detail for {id}: {e:#}");
            }
            results.push((id.clone(), result));
            progress.inc(1);
            self.throttle().await;
        }

        results
    }
}
