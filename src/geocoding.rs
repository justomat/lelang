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