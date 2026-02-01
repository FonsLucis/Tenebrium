Verify vectors (cross-language)

Purpose: provide a simple Python script to verify that other implementations will produce the same canonical bytes and txid (v1 and v2) as the Rust implementation.

How to run:
- Ensure Python 3.8+ is available.
- From repository root:
  python tools/verify_vectors.py

Notes:
- The script reads `crates/tenebrium-utxo/test_vectors/vectors.json` (UTF-8, BOM tolerated).
- No external packages required (only Python stdlib).
