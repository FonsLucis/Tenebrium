# Test Vectors

이 디렉토리는 Tenebrium UTXO 직렬화/해시 벡터를 저장합니다.

## 파일
- vectors.json: Rust 구현 기준 원본 벡터
- vectors_cross_language.json: 타 언어 구현을 위한 보조 벡터(문자열/hex 포함)

## vectors_cross_language.json 스키마
각 항목은 다음 필드를 포함합니다.
- name: 벡터 이름
- tx: 트랜잭션 JSON
- canonical_v1_json: v1 canonical JSON 문자열
- canonical_v1_hex: v1 canonical JSON의 hex
- canonical_v2_hex: v2 canonical bytes의 hex
- txid_v1_hex: v1 txid (double-SHA256) hex
- txid_v2_hex: v2 txid (double-SHA256) hex

## 생성 방법 (Rust)
```powershell
cargo run -p tenebrium-utxo --example export_vectors_cross_language
```
