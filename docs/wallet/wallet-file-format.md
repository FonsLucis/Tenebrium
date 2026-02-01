# Wallet File Format (v1)

Tenebrium wallet file is a JSON document that stores an encrypted secret key and derived metadata.

## Overview
- KDF: scrypt
- Cipher: AES-256-GCM
- Secret key length: 32 bytes (ed25519)
- Address: Bech32 using HRP `tn`

## JSON Schema (informal)
```json
{
  "version": 1,
  "kdf": "scrypt",
  "kdf_params": {
    "salt_hex": "<hex>",
    "n": 32768,
    "r": 8,
    "p": 1
  },
  "cipher": "aes-256-gcm",
  "nonce_hex": "<12-byte hex>",
  "ciphertext_hex": "<hex>",
  "public_key_hex": "<32-byte hex>",
  "address": "tn1..."
}
```

## Field Notes
- `ciphertext_hex` stores the encrypted 32-byte secret key.
- `public_key_hex` and `address` are stored for quick reference and are validated on load.
- `kdf_params.n` must be a power of two.

## CLI Usage
- Save (hidden input if `--passphrase` omitted):
  - `tenebrium-cli wallet save --secret <hex> --out <path>`
- Load (hidden input if `--passphrase` omitted):
  - `tenebrium-cli wallet load --input <path>`

## Test Vectors
- `docs/wallet/wallet-vectors.json`

## Backup / Restore
- Backup (re-encrypt with a new passphrase):
  - `tenebrium-cli wallet backup --input <path> --out <path>`
  - Prompts for current passphrase and new passphrase (with confirmation)
- Restore: use `wallet load` to decrypt and recover keys

## Versioning Strategy
- `version` is required.
- Backward-compatible changes only (additive fields) within the same major version.
- Breaking changes must bump `version` and be explicitly handled in the loader.
