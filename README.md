# Tenebrium

Rust 기반 블록체인/노드 프로젝트입니다. UTXO 트랜잭션/직렬화, 합의 규칙, 노드 데몬, CLI로 구성됩니다.

## 구성
- crates/tenebrium-core: 공통 타입/유틸/암호화 추상화
- crates/tenebrium-utxo: 트랜잭션 구조, canonicalization(v1 JSON, v2 binary), UTXO 검증
- crates/tenebrium-consensus: 블록 유효성/PoW/합의 규칙(타깃 블록 시간 600s)
- crates/tenebriumd: 노드/데몬 런타임
- crates/tenebrium-cli: CLI

## 핵심 데이터 흐름
Transaction -> canonical_bytes_v2 -> double-SHA256 -> txid_v2

## 빌드/테스트
- cargo fmt --all -- --check
- cargo test --all --workspace --verbose

## 벡터 검증
- python tools/verify_vectors.py

## 벡터 생성
- cargo run -p tenebrium-utxo --example generate_vectors > crates/tenebrium-utxo/test_vectors/generated_vectors.json
- python tools/verify_vectors.py

## 참고 문서
- crates/tenebrium-utxo/CANONICAL_TXID_V2.md
- docs/rfcs/
- tools/README-verify-vectors.md
