use ed25519_dalek::{Signer, SigningKey};
use std::env;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = env::args().nth(1).ok_or("usage: sign_manifest <manifest.json> <signature> <public-key>")?;
    let signature = env::args().nth(2).ok_or("usage: sign_manifest <manifest.json> <signature> <public-key>")?;
    let public = env::args().nth(3).ok_or("usage: sign_manifest <manifest.json> <signature> <public-key>")?;
    let key = env::var("PHOTOCRAFT_UPDATE_SIGNING_KEY")?;
    let bytes = hex::decode(key.trim())?;
    let raw: [u8; 32] = bytes.try_into().map_err(|_| "PHOTOCRAFT_UPDATE_SIGNING_KEY must contain 32 bytes as hex")?;
    let signing = SigningKey::from_bytes(&raw);
    let manifest = fs::read(input)?;
    fs::write(signature, hex::encode(signing.sign(&manifest).to_bytes()))?;
    fs::write(public, hex::encode(signing.verifying_key().to_bytes()))?;
    Ok(())
}
