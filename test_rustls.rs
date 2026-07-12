use reqwest::Client;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .use_rustls_tls()
        .http1_only() // Disable H2 to avoid HTTP/2 fingerprinting issues
        .build()?;
    let resp = client.get("https://api-auth.lelang.go.id/api/v1/master/ref-provinsi?limit=1")
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;
    println!("Status: {}", resp.status());
    Ok(())
}
