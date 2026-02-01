# Transaction JSON Format (CLI)

Tenebrium CLI uses a hex-friendly JSON format for transaction creation and signing.

## Schema (informal)
```json
{
  "version": 1,
  "lock_time": 0,
  "vin": [
    {
      "prevout": {
        "txid_hex": "<32-byte hex>",
        "vout": 0
      },
      "sequence": 4294967295,
      "script_sig_hex": "<hex>" 
    }
  ],
  "vout": [
    {
      "value": 1000,
      "script_pubkey_hex": "<hex>"
    }
  ]
}
```

Notes:
- `script_sig_hex` is optional on input. If omitted, it is treated as empty.
- `script_pubkey_hex` is required for outputs.

## Signing Rules (v0.1 tooling)
- Sighash: double-SHA256 of canonical bytes v2 with **all `script_sig` cleared**.
- Signature: ed25519 signature over sighash.
- `script_sig` layout: `signature(64 bytes) || public_key(32 bytes)`.

## CLI Examples
- Create (unsigned):
  - `tenebrium-cli tx create --input tx_template.json`
- Create + sign:
  - `tenebrium-cli tx create --input tx_template.json --sign-secret <hex>`
- Sign existing tx:
  - `tenebrium-cli tx sign --input tx.json --secret <hex>`
- Info:
  - `tenebrium-cli tx info --input tx.json`
- Build from UTXO JSONL:
  - `tenebrium-cli tx build --utxo utxo.jsonl --to-script <hex> --amount 1000 --change-script <hex> --fee 10`
  - `tenebrium-cli tx build --utxo utxo.jsonl --to-script <hex> --amount 1000 --change-script <hex> --fee-rate 1 --strategy largest-first`

## Coin Selection
- `largest-first` (기본값): 큰 UTXO부터 선택
- `smallest-first`: 작은 UTXO부터 선택
- `random`: 무작위 선택
- `best-fit`: 1개 UTXO로 충족 가능한 경우 잔돈이 가장 적은 항목 선택 (그 외는 largest-first)

## Fee Estimation
- `--fee` 또는 `--fee-rate` 중 하나만 사용
- `--fee-rate`는 v2 canonical 기준 추정 크기에서 계산
- 서명 시 스크립트 시그니처 길이 96 bytes (signature 64 + pubkey 32), 미서명은 0 bytes
- 잔돈 출력 유무(0/1개)를 고려해 추정
