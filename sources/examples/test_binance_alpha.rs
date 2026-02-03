use oracle_ark_sources::parsers;

fn main() {
    let json: serde_json::Value = serde_json::json!({
        "code": "000000",
        "data": [
            {
                "contractAddress": "0x4c067de26475e1cefee8b8d1f6e2266b33a2372e",
                "price": "0.012131803081106568831",
                "symbol": "RHEA"
            }
        ]
    });
    
    match parsers::parse_binance_alpha(&json, "0x4c067de26475e1cefee8b8d1f6e2266b33a2372e") {
        Ok(price) => println!("SUCCESS: price = {}", price),
        Err(e) => println!("ERROR: {}", e),
    }
}
