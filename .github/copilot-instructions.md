# Tenebrium Copilot Instructions

> 간결 가이드: 이 파일은 리포지토리에서 즉시 생산적으로 작업하려는 AI 에이전트를 위한 실행 가능한 규칙과 코드베이스 중심 참조입니다. 예제 파일/명령어와 함께 구체적으로 적어 두었습니다.

## 한눈에 보기 (아키텍처)
- Workspace 멤버: `crates/tenebrium-core`, `crates/tenebrium-utxo`, `crates/tenebrium-consensus`, `crates/tenebriumd`, `crates/tenebrium-cli`.
- 책임 분배 (요약):
  - `tenebrium-core`: 공통 타입/유틸/암호화 추상화
  - `tenebrium-utxo`: 트랜잭션 구조, canonicalization (v1 JSON, v2 binary), UTXO 집합 및 검증
  - `tenebrium-consensus`: 블록 유효성/PoW/합의 규칙 (타깃 블록 시간 600s)
  - `tenebriumd`: 데몬/노드 실행 코드
  - `tenebrium-cli`: 명령행 클라이언트
- 주요 데이터 흐름: 트랜잭션 -> `canonical_bytes_v2` (deterministic binary) -> double-SHA256 -> `txid_v2`. canonical v1은 JSON 포맷(serde_json::to_vec)으로 호환 유지.

## 중요한 파일(즉시 참고)
- 트랜잭션/UTXO 구현: `crates/tenebrium-utxo/src/lib.rs` (상수: `MAX_SCRIPT_SIZE`, `MAX_TX_INOUTS`, `Transaction::canonical_bytes_v2` 레이아웃)
- 벡터 생성 예제: `crates/tenebrium-utxo/examples/generate_vectors.rs`
- 벡터 검증 스크립트: `tools/verify_vectors.py` (Python으로 canonical v1/v2과 txid를 재계산)
- CI 구현: `.github/workflows/ci.yml` (포맷 체크, `cargo test`, 벡터 생성/비교, 자동 PR)
- RFCs / 설계: `docs/rfcs/*.md`, `crates/tenebrium-utxo/CANONICAL_TXID_V2.md`

## 로컬 개발 & CI에서 사용하는 명령(꼭 외워두기) ✅
- 포맷 검사: `cargo fmt --all -- --check`
- 빌드/테스트(워크스페이스): `cargo test --all --workspace --verbose`
- 벡터 검증(로컬): `python tools/verify_vectors.py`
- 벡터 생성(예제 실행):
  - `cargo run -p tenebrium-utxo --example generate_vectors > crates/tenebrium-utxo/test_vectors/generated_vectors.json`
  - 그 뒤 `tools/verify_vectors.py` 또는 CI와 동일한 비교 로직으로 확인
- CI 특이사항: 벡터가 변경되면 CI가 `auto/update-test-vectors-<run_id>-<changed_count>` 브랜치와 `chore(test-vectors): update <N> vectors` 커밋을 포함해 PR을 자동으로 생성합니다.

## 코드/스타일 규칙(프로젝트 특성) 🔧
- Rust stable (toolchain override in CI), Edition 2021.
- 보안/무결성 경계: consensus, parser, network 입력 처리부에서는 `unwrap()`/`expect()` 사용 금지 — 항상 `Result`로 에러를 반환하고 호출자에게 전파.
- 입력 검증은 모듈 경계에서 반드시 수행 (길이/범위/형식). 예: `Transaction::validate()`는 canonicalization 전에 호출되어야 함.
- 에러 타입: 도메인별 `enum` 사용 (`UtxoError` 등). 가능한 `thiserror`를 이용해 명확한 메시지 제공.
- 암호화: 직접 구현 금지 — 신뢰된 크레이트 사용.
- 모듈은 가급적 500 라인 이하로 유지.

## 벡터와 직렬화 주의사항 (특히 중요) ⚠️
- v2 canonical: binary layout (리틀 엔디언 정수, 길이 필드 u64 등) — `crates/tenebrium-utxo/src/lib.rs`에 명세가 있음.
- v1 canonical은 `serde_json::to_vec` 출력과 바이트 동일성이 필요합니다. (Python 검증은 `json.dumps(..., separators=(",",":"), ensure_ascii=False)`로 동일 포맷을 재현합니다.)
- txid는 canonical bytes v2의 double-SHA256입니다. `tools/verify_vectors.py`와 `examples/generate_vectors.rs`를 직접 확인하세요.

## 작업/리뷰 시 포맷 및 PR 규칙
- 커밋/PR은 CI가 통과하도록 유지: `cargo fmt`/`cargo test`/`tools/verify_vectors.py`를 로컬에서 먼저 실행하세요.
- 벡터를 변경해야 하는 수정은 다음을 포함:
  1. `cargo run -p tenebrium-utxo --example generate_vectors`로 `generated_vectors.json` 생성
  2. `tools/verify_vectors.py`로 검증
  3. 변경을 커밋하고 PR을 올리면 CI가 동일 검증을 재실행합니다.

## Deliverable 템플릿 (작업 시작/종료 시 사용)
- 시작: `TASK: <one-line>` / `PLAN: 1) ... 2) ...`
- 종료 (PR 본문에 포함):
  - `CHANGED FILES:` 목록
  - `HOW TO RUN:` 빌드/테스트/검증 명령
  - `RISKS / NEXT:` 2~5가지 우선순위

---
피드백 원하시면, 어느 부분(예: 벡터 생성 절차, consensus 제약, 혹은 특정 파일 참조)을 더 상세히 문서화할지 알려주세요. ✉️
