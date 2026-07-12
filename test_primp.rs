use primp::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .impersonate(primp::Impersonate::Chrome130)
        .build()?;
    
    let resp = client.get("https://api-auth.lelang.go.id/api/v1/master/ref-provinsi?limit=1")
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await?;
        
    println!("Status: {}", resp.status());
    let text = resp.text().await?;
    println!("Body: {:.100}", text);
    Ok(())
}
