fn main() {
    let agent = ureq::builder().build();
    let resp = agent.get("https://api-auth.lelang.go.id/api/v1/master/ref-provinsi?limit=1")
        .set("User-Agent", "Mozilla/5.0")
        .call();
    println!("{:?}", resp.map(|r| r.status()));
}
