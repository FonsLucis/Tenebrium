# Sled DB 스키마

## 경로
- data_dir/chain.sled

## Trees
- meta
- headers
- heights
- work
- utxo
- blocks

## meta 키
- schema_version (u32, le)
- network_id (string bytes)
- tip_hash (32 bytes)
- tip_height (u32, le)
- utxo_count (u64, le)

## headers
- key: block hash (32 bytes)
- value: BlockHeader JSON

## heights
- key: block hash (32 bytes)
- value: height (u32, le)

## work
- key: block hash (32 bytes)
- value: chain work (u128, le)

## blocks
- key: block hash (32 bytes)
- value: Block JSON

## utxo
- key: outpoint (txid[32] + vout[u32 le])
- value: TxOut (value[u64 le] + script_len[u64 le] + script bytes)

## 마이그레이션
- `db-migrate`로 schema_version 업데이트
- `db-migrate --dry-run --json`으로 사전 검증 가능
