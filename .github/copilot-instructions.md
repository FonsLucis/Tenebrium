# Tenebrium Copilot Instructions

> 이 문서는 이 리포지토리에서 즉시 생산적으로 작업하기 위한 “사실 기반” 가이드를 제공합니다.

## Big Picture (구조/흐름)
- Workspace crates: `crates/tenebrium-core`, `crates/tenebrium-utxo`, `crates/tenebrium-consensus`, `crates/tenebriumd`, `crates/tenebrium-cli`.
- 책임 분리:
  - `tenebrium-core`: 공통 타입/유틸/암호화 추상화
  - `tenebrium-utxo`: 트랜잭션 구조, canonicalization(v1 JSON, v2 binary), UTXO 검증
  - `tenebrium-consensus`: 블록 유효성/PoW/합의 규칙(타깃 블록 시간 600s)
  - `tenebriumd`: 노드/데몬 런타임
  - `tenebrium-cli`: CLI
- 핵심 데이터 흐름: `Transaction::canonical_bytes_v2` → double-SHA256 → `txid_v2`.
  - v1 canonical은 `serde_json::to_vec`의 바이트와 동일해야 함.

## 반드시 보는 파일
- canonical/UTXO 핵심: `crates/tenebrium-utxo/src/lib.rs` (상수 `MAX_SCRIPT_SIZE`, `MAX_TX_INOUTS`, v2 레이아웃)
- 벡터 생성: `crates/tenebrium-utxo/examples/generate_vectors.rs`
- 교차언어 벡터: `crates/tenebrium-utxo/examples/export_vectors_cross_language.rs`, `crates/tenebrium-utxo/test_vectors/README.md`
- 벡터 검증: `tools/verify_vectors.py`
- 설계 문서: `crates/tenebrium-utxo/CANONICAL_TXID_V2.md`, `docs/rfcs/*.md`

## 필수 워크플로 (CI와 동일 기준)
- 포맷: `cargo fmt --all -- --check`
- 테스트: `cargo test --all --workspace --verbose`
- 벡터 검증: `python tools/verify_vectors.py`
- 벡터 생성(변경 시 필수):
  - `cargo run -p tenebrium-utxo --example generate_vectors > crates/tenebrium-utxo/test_vectors/generated_vectors.json`
  - 이후 `tools/verify_vectors.py`로 확인
- CI는 벡터 변경 시 자동 PR 생성(브랜치 `auto/update-test-vectors-<run_id>-<changed_count>`)

## 프로젝트 특화 규칙
- Rust stable, Edition 2021.
- consensus/parser/network 경계에서는 `unwrap()`/`expect()` 금지 → `Result`로 전파.
- 입력 검증은 모듈 경계에서 수행 (예: `Transaction::validate()` 후 canonicalization).
- 에러는 도메인별 `enum`(예: `UtxoError`) + `thiserror` 패턴을 선호.
- 암호화는 직접 구현 금지(신뢰된 크레이트 사용).


## 기억
레포 루트에 STATUS.md를 만들고, 앞으로 모든 작업 종료 시 반드시 갱신해라.
형식:
Done (체크리스트)
Next (Top 3)
How to run (commands)
Notes (깨진 것/리스크)

## Deliverable 템플릿
- 시작: `TASK: <one-line>` / `PLAN: 1) ... 2) ...`
- 종료:
  - `CHANGED FILES:`
  - `HOW TO RUN:`
  - `RISKS / NEXT:`
