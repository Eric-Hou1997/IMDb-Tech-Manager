fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: verify_update PUBLIC_KEY SIGNATURE ARTIFACT".into());
    }
    let public = std::fs::read_to_string(&args[0])?;
    let signature = std::fs::read_to_string(&args[1])?;
    let bytes = std::fs::read(&args[2])?;
    itm_core::update::verify_signature(&public, &signature, &bytes)?;
    let mut corrupted = bytes;
    corrupted.push(0);
    assert!(itm_core::update::verify_signature(&public, &signature, &corrupted).is_err());
    println!("Existing Ed25519 key accepted in Tauri Minisign format; tampered artifact rejected");
    Ok(())
}
