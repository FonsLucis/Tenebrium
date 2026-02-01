use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use bech32::{ToBase32, Variant};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use scrypt::{scrypt, Params as ScryptParams};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ADDRESS_HRP: &str = "tn";

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("invalid secret key length: {0}")]
    InvalidSecretLength(usize),
    #[error("invalid public key length: {0}")]
    InvalidPublicKeyLength(usize),
    #[error("invalid signature length: {0}")]
    InvalidSignatureLength(usize),
    #[error("bech32 error: {0}")]
    Bech32(#[from] bech32::Error),
    #[error("signature error: {0}")]
    Signature(#[from] ed25519_dalek::SignatureError),
    #[error("scrypt error: {0}")]
    Scrypt(#[from] scrypt::errors::InvalidParams),
    #[error("scrypt output length error: {0}")]
    ScryptOutputLen(#[from] scrypt::errors::InvalidOutputLen),
    #[error("aes-gcm error")]
    AesGcm,
    #[error("invalid wallet file: {0}")]
    InvalidWalletFile(String),
}

pub struct WalletKeypair {
    signing_key: SigningKey,
}

impl WalletKeypair {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    pub fn from_secret_hex(secret_hex: &str) -> Result<Self, WalletError> {
        let bytes = hex::decode(secret_hex)?;
        if bytes.len() != 32 {
            return Err(WalletError::InvalidSecretLength(bytes.len()));
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self {
            signing_key: SigningKey::from_bytes(&array),
        })
    }

    pub fn secret_key_hex(&self) -> String {
        hex::encode(self.signing_key.to_bytes())
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().as_bytes())
    }

    pub fn address(&self) -> Result<String, WalletError> {
        address_from_pubkey(&self.signing_key.verifying_key())
    }

    pub fn sign_message(&self, message: &[u8]) -> String {
        let sig = self.signing_key.sign(message);
        hex::encode(sig.to_bytes())
    }

    pub fn to_wallet_file(&self, passphrase: &str) -> Result<WalletFile, WalletError> {
        wallet_file_from_secret(&self.secret_key_hex(), passphrase)
    }
}

pub fn generate_keypair() -> WalletKeypair {
    WalletKeypair::generate()
}

pub fn address_from_pubkey_hex(pubkey_hex: &str) -> Result<String, WalletError> {
    let bytes = hex::decode(pubkey_hex)?;
    if bytes.len() != 32 {
        return Err(WalletError::InvalidPublicKeyLength(bytes.len()));
    }
    let mut array = [0u8; 32];
    array.copy_from_slice(&bytes);
    let verifying_key = VerifyingKey::from_bytes(&array)?;
    address_from_pubkey(&verifying_key)
}

pub fn sign_message_hex(secret_hex: &str, message: &[u8]) -> Result<String, WalletError> {
    let kp = WalletKeypair::from_secret_hex(secret_hex)?;
    Ok(kp.sign_message(message))
}

pub fn verify_message_hex(
    pubkey_hex: &str,
    message: &[u8],
    signature_hex: &str,
) -> Result<bool, WalletError> {
    let pub_bytes = hex::decode(pubkey_hex)?;
    if pub_bytes.len() != 32 {
        return Err(WalletError::InvalidPublicKeyLength(pub_bytes.len()));
    }
    let mut pub_array = [0u8; 32];
    pub_array.copy_from_slice(&pub_bytes);
    let verifying_key = VerifyingKey::from_bytes(&pub_array)?;

    let sig_bytes = hex::decode(signature_hex)?;
    if sig_bytes.len() != 64 {
        return Err(WalletError::InvalidSignatureLength(sig_bytes.len()));
    }
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_array);

    Ok(verifying_key.verify(message, &signature).is_ok())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletFile {
    pub version: u32,
    pub kdf: String,
    pub kdf_params: KdfParams,
    pub cipher: String,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
    pub public_key_hex: String,
    pub address: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KdfParams {
    pub salt_hex: String,
    pub n: u32,
    pub r: u32,
    pub p: u32,
}

pub fn wallet_file_from_secret(
    secret_hex: &str,
    passphrase: &str,
) -> Result<WalletFile, WalletError> {
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);
    wallet_file_from_secret_with_kdf(secret_hex, passphrase, &salt, &nonce_bytes, 1 << 15, 8, 1)
}

pub fn wallet_file_from_secret_with_kdf(
    secret_hex: &str,
    passphrase: &str,
    salt: &[u8],
    nonce: &[u8],
    n: u32,
    r: u32,
    p: u32,
) -> Result<WalletFile, WalletError> {
    let kp = WalletKeypair::from_secret_hex(secret_hex)?;
    let public_key_hex = kp.public_key_hex();
    let address = kp.address()?;

    if n == 0 || !n.is_power_of_two() {
        return Err(WalletError::InvalidWalletFile("invalid N".to_string()));
    }
    let n_log2 = n.trailing_zeros();
    let params = ScryptParams::new(n_log2 as u8, r, p, 32)?;
    let mut key = [0u8; 32];
    scrypt(passphrase.as_bytes(), salt, &params, &mut key)?;

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| WalletError::AesGcm)?;
    if nonce.len() != 12 {
        return Err(WalletError::InvalidWalletFile(
            "invalid nonce length".to_string(),
        ));
    }
    let nonce = Nonce::from_slice(nonce);
    let secret_bytes = hex::decode(secret_hex)?;
    if secret_bytes.len() != 32 {
        return Err(WalletError::InvalidSecretLength(secret_bytes.len()));
    }
    let ciphertext = cipher
        .encrypt(nonce, secret_bytes.as_ref())
        .map_err(|_| WalletError::AesGcm)?;

    Ok(WalletFile {
        version: 1,
        kdf: "scrypt".to_string(),
        kdf_params: KdfParams {
            salt_hex: hex::encode(salt),
            n,
            r,
            p,
        },
        cipher: "aes-256-gcm".to_string(),
        nonce_hex: hex::encode(nonce),
        ciphertext_hex: hex::encode(ciphertext),
        public_key_hex,
        address,
    })
}

pub fn wallet_keypair_from_file(
    wallet: &WalletFile,
    passphrase: &str,
) -> Result<WalletKeypair, WalletError> {
    if wallet.version != 1 {
        return Err(WalletError::InvalidWalletFile(
            "unsupported version".to_string(),
        ));
    }
    if wallet.kdf != "scrypt" || wallet.cipher != "aes-256-gcm" {
        return Err(WalletError::InvalidWalletFile(
            "unsupported kdf or cipher".to_string(),
        ));
    }

    let salt = hex::decode(&wallet.kdf_params.salt_hex)?;
    let nonce = hex::decode(&wallet.nonce_hex)?;
    let ciphertext = hex::decode(&wallet.ciphertext_hex)?;
    if nonce.len() != 12 {
        return Err(WalletError::InvalidWalletFile(
            "invalid nonce length".to_string(),
        ));
    }
    let n = wallet.kdf_params.n;
    if n == 0 || !n.is_power_of_two() {
        return Err(WalletError::InvalidWalletFile("invalid N".to_string()));
    }
    let n_log2 = n.trailing_zeros();
    let params = ScryptParams::new(n_log2 as u8, wallet.kdf_params.r, wallet.kdf_params.p, 32)?;
    let mut key = [0u8; 32];
    scrypt(passphrase.as_bytes(), &salt, &params, &mut key)?;

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| WalletError::AesGcm)?;
    let nonce = Nonce::from_slice(&nonce);
    let secret_bytes = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| WalletError::AesGcm)?;
    if secret_bytes.len() != 32 {
        return Err(WalletError::InvalidSecretLength(secret_bytes.len()));
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&secret_bytes);
    let kp = WalletKeypair {
        signing_key: SigningKey::from_bytes(&secret),
    };

    if kp.public_key_hex() != wallet.public_key_hex {
        return Err(WalletError::InvalidWalletFile(
            "public key mismatch".to_string(),
        ));
    }
    if kp.address()? != wallet.address {
        return Err(WalletError::InvalidWalletFile(
            "address mismatch".to_string(),
        ));
    }

    Ok(kp)
}

pub fn wallet_file_reencrypt(
    wallet: &WalletFile,
    old_passphrase: &str,
    new_passphrase: &str,
) -> Result<WalletFile, WalletError> {
    let kp = wallet_keypair_from_file(wallet, old_passphrase)?;
    wallet_file_from_secret(&kp.secret_key_hex(), new_passphrase)
}

fn address_from_pubkey(pubkey: &VerifyingKey) -> Result<String, WalletError> {
    let hash = Sha256::digest(pubkey.as_bytes());
    let addr = bech32::encode(ADDRESS_HRP, hash.to_base32(), Variant::Bech32)?;
    Ok(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn address_roundtrip_from_secret() {
        let secret = [1u8; 32];
        let kp = WalletKeypair::from_secret_hex(&hex::encode(secret)).unwrap();
        let addr1 = kp.address().unwrap();
        let addr2 = address_from_pubkey_hex(&kp.public_key_hex()).unwrap();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn sign_and_verify() {
        let secret = [2u8; 32];
        let kp = WalletKeypair::from_secret_hex(&hex::encode(secret)).unwrap();
        let message = b"hello";
        let sig = kp.sign_message(message);
        let ok = verify_message_hex(&kp.public_key_hex(), message, &sig).unwrap();
        assert!(ok);
    }

    #[test]
    fn wallet_file_encrypt_decrypt() {
        let secret = [3u8; 32];
        let kp = WalletKeypair::from_secret_hex(&hex::encode(secret)).unwrap();
        let wallet = kp.to_wallet_file("pass").unwrap();
        let kp2 = wallet_keypair_from_file(&wallet, "pass").unwrap();
        assert_eq!(kp.public_key_hex(), kp2.public_key_hex());
    }

    #[test]
    fn wallet_file_reencrypt_test() {
        let secret = [4u8; 32];
        let kp = WalletKeypair::from_secret_hex(&hex::encode(secret)).unwrap();
        let wallet = kp.to_wallet_file("old").unwrap();
        let wallet2 = wallet_file_reencrypt(&wallet, "old", "new").unwrap();
        let kp2 = wallet_keypair_from_file(&wallet2, "new").unwrap();
        assert_eq!(kp.public_key_hex(), kp2.public_key_hex());
    }

    #[test]
    fn wallet_vector_matches() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("..");
        path.push("..");
        path.push("docs");
        path.push("wallet");
        path.push("wallet-vectors.json");

        let data = std::fs::read_to_string(path).unwrap();
        let data = data.trim_start_matches('\u{feff}');
        let wallet: WalletFile = serde_json::from_str(data).unwrap();

        assert_eq!(wallet.version, 1);
        assert_eq!(wallet.kdf, "scrypt");
        assert_eq!(wallet.cipher, "aes-256-gcm");
        assert_eq!(wallet.kdf_params.n, 32768);
        assert_eq!(wallet.kdf_params.r, 8);
        assert_eq!(wallet.kdf_params.p, 1);
        assert_eq!(
            wallet.kdf_params.salt_hex,
            "000102030405060708090a0b0c0d0e0f"
        );
        assert_eq!(wallet.nonce_hex, "0f0e0d0c0b0a090807060504");
        assert_eq!(
            wallet.ciphertext_hex,
            "3290608a78fc45997c4643fe701910c441417ba80db5770c43a245df0203b3dac1218a6869936934da79f5e93c4e3ee1"
        );
        assert_eq!(
            wallet.public_key_hex,
            "03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8"
        );
        assert_eq!(
            wallet.address,
            "tn12er44f65vdr5cq59mawm7272ku76v5f43qu7ndm5sxew4vg8wzxqdjnd7h"
        );

        let kp = wallet_keypair_from_file(&wallet, "correct horse battery staple").unwrap();
        assert_eq!(kp.public_key_hex(), wallet.public_key_hex);
    }
}
