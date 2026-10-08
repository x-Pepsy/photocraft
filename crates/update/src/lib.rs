//! Signed release metadata shared by packaging and the desktop updater.
//!
//! The manifest is intentionally independent of HTTP and the desktop shell. Callers fetch bytes
//! from GitHub Releases, then verify the signature and select an asset for their platform.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema: u32,
    pub version: String,
    pub channel: Channel,
    pub published_at: Option<String>,
    pub minimum_supported_version: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Preview,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Asset {
    pub platform: String,
    pub arch: String,
    pub kind: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Json(String),
    UnsupportedSchema(u32),
    InvalidVersion(String),
    InvalidHash(String),
    InvalidPublicKey(String),
    InvalidSignature(String),
    SignatureMismatch,
    AssetNotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid update manifest JSON: {e}"),
            Self::UnsupportedSchema(v) => write!(f, "unsupported update manifest schema {v}"),
            Self::InvalidVersion(v) => write!(f, "invalid release version `{v}`"),
            Self::InvalidHash(h) => write!(f, "invalid SHA-256 `{h}`"),
            Self::InvalidPublicKey(e) => write!(f, "invalid update public key: {e}"),
            Self::InvalidSignature(e) => write!(f, "invalid update signature: {e}"),
            Self::SignatureMismatch => write!(f, "update manifest signature mismatch"),
            Self::AssetNotFound => write!(f, "no update asset matches this platform"),
        }
    }
}

impl std::error::Error for Error {}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let manifest: Self = serde_json::from_slice(bytes).map_err(|e| Error::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != SCHEMA {
            return Err(Error::UnsupportedSchema(self.schema));
        }
        validate_version(&self.version)?;
        if let Some(version) = &self.minimum_supported_version {
            validate_version(version)?;
        }
        for asset in &self.assets {
            if asset.file.is_empty() || asset.file.contains('/') || asset.file.contains('\\') || asset.file == "." || asset.file == ".." {
                return Err(Error::InvalidHash(asset.file.clone()));
            }
            if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::InvalidHash(asset.sha256.clone()));
            }
        }
        Ok(())
    }

    pub fn asset(&self, platform: &str, arch: &str, kind: &str) -> Result<&Asset, Error> {
        self.assets.iter().find(|a| a.platform == platform && a.arch == arch && a.kind == kind).ok_or(Error::AssetNotFound)
    }
}

pub fn verify_manifest(bytes: &[u8], signature_hex: &str, public_key_hex: &str) -> Result<Manifest, Error> {
    let key_bytes = hex::decode(public_key_hex).map_err(|e| Error::InvalidPublicKey(e.to_string()))?;
    let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| Error::InvalidPublicKey("expected 32 bytes".into()))?;
    let key = VerifyingKey::from_bytes(&key_array).map_err(|e| Error::InvalidPublicKey(e.to_string()))?;
    let sig_bytes = hex::decode(signature_hex).map_err(|e| Error::InvalidSignature(e.to_string()))?;
    let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| Error::InvalidSignature("expected 64 bytes".into()))?;
    key.verify(bytes, &Signature::from_bytes(&sig_array)).map_err(|_| Error::SignatureMismatch)?;
    Manifest::parse(bytes)
}

pub fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn validate_version(version: &str) -> Result<(), Error> {
    let base = version.strip_prefix('v').unwrap_or(version).split_once('-').map_or(version, |(base, _)| base);
    let mut parts = base.split('.');
    let valid = (0..3).all(|_| parts.next().is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))) && parts.next().is_none();
    if valid { Ok(()) } else { Err(Error::InvalidVersion(version.into())) }
}

pub fn newer_than(candidate: &str, current: &str) -> Result<bool, Error> {
    let parse = |v: &str| -> Result<[u64; 3], Error> {
        let base = v.strip_prefix('v').unwrap_or(v).split_once('-').map_or(v, |(base, _)| base);
        let mut p = base.split('.');
        let mut out = [0; 3];
        for slot in &mut out {
            *slot = p.next().ok_or_else(|| Error::InvalidVersion(v.into()))?.parse().map_err(|_| Error::InvalidVersion(v.into()))?;
        }
        if p.next().is_some() {
            return Err(Error::InvalidVersion(v.into()));
        }
        Ok(out)
    };
    Ok(parse(candidate)? > parse(current)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn manifest() -> Vec<u8> {
        serde_json::to_vec(&Manifest {
            schema: SCHEMA,
            version: "1.2.3".into(),
            channel: Channel::Stable,
            published_at: None,
            minimum_supported_version: None,
            assets: vec![Asset {
                platform: "linux".into(),
                arch: "x86_64".into(),
                kind: "appimage".into(),
                file: "PhotoCraft.AppImage".into(),
                sha256: "0".repeat(64),
                size: 12,
            }],
        })
        .unwrap_or_default()
    }

    #[test]
    fn verifies_signed_manifest_and_hashes() {
        let bytes = manifest();
        let signing = SigningKey::from_bytes(&[7; 32]);
        let sig = signing.sign(&bytes);
        let parsed = verify_manifest(&bytes, &hex::encode(sig.to_bytes()), &hex::encode(signing.verifying_key().to_bytes())).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.version, "1.2.3");
        assert_eq!(sha256(b"test"), "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
    }

    #[test]
    fn rejects_path_traversal_and_bad_versions() {
        let mut value: serde_json::Value = serde_json::from_slice(&manifest()).unwrap_or_default();
        value["assets"][0]["file"] = serde_json::Value::String("../payload".into());
        assert!(matches!(Manifest::parse(&serde_json::to_vec(&value).unwrap_or_default()), Err(Error::InvalidHash(_))));
        assert!(!newer_than("1.2.3", "1.2.3").unwrap_or(true));
        assert!(newer_than("1.2.4", "1.2.3").unwrap_or(false));
    }
}
