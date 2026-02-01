use tenebrium_core::wallet_file_from_secret_with_kdf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let secret_hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    let passphrase = "correct horse battery staple";
    let salt = hex::decode("000102030405060708090a0b0c0d0e0f")?;
    let nonce = hex::decode("0f0e0d0c0b0a090807060504")?;
    let wallet =
        wallet_file_from_secret_with_kdf(secret_hex, passphrase, &salt, &nonce, 32768, 8, 1)?;
    let json = serde_json::to_string_pretty(&wallet)?;
    println!("{json}");
    Ok(())
}
