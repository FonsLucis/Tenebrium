# RFC 0001 — Canonical TXID v2 (Tenebrium)

- Status: Draft
- Authors: Tenebrium Contributors
- Date: 2026-02-01
- Related: `crates/tenebrium-utxo/CANONICAL_TXID_V2.md`

## 개요 (Abstract)

이 문서는 Tenebrium 프로젝트에서 도입하려는 **Canonical TXID v2**의 공식 사양을 제시한다. 본 규격은 트랜잭션의 결정적(deteministic) 바이너리 직렬화 형식과 그 바이트열에 대한 double-SHA256 해시를 txid로 정의한다. 목적은 언어/플랫폼 간 일관된 txid 계산을 보장하고, 성능·저장 효율·분석의 신뢰성을 향상시키는 것이다.

## 동기 (Motivation)

- JSON 기반 v1은 사람 친화적이나 바이트 수준 표현이 명확하지 않아 txid 산출 시 언어/라이브러리별 미세차가 발생할 수 있다.
- 대규모 서비스에서 직렬화 성능 및 네트워크/디스크 효율 향상이 요구된다.
- 결정적 바이너리 포맷은 교차-언어 테스트 벡터 생성과 자동 검증을 용이하게 한다.

## 요구사항 (Requirements)

1. 모든 구현 언어에서 동일한 바이트열을 생성할 수 있어야 한다.
2. txid는 바이트열의 double-SHA256으로 정의한다.
3. 포맷은 명확한 길이-접두사(length-prefixed) 방식으로 파싱 애매모호성을 제거한다.
4. 향후 확장을 고려해 버전화를 지원할 수 있어야 한다.

## 규격 (Specification)

### 전반 규칙
- 모든 정수는 **little-endian**으로 인코딩한다.
- 가변길이 바이트 배열(스크립트 등)은 `u64`(8바이트, 리틀엔디안) 길이 접두사 + 바이트 데이터로 인코딩한다.
- 바이트 레이아웃은 다음 순서를 따른다.

### 필드 레이아웃 (정확한 바이트 순서)

1. version: i32 (4 bytes, little-endian)
2. vin_count: u64 (8 bytes)
3. for each vin (in order):
   - prevout.txid: 32 bytes (raw, no byte-order change)
   - prevout.vout: u32 (4 bytes)
   - script_sig_len: u64 (8 bytes)
   - script_sig: [script_sig_len] bytes
   - sequence: u32 (4 bytes)
4. vout_count: u64 (8 bytes)
5. for each vout (in order):
   - value: u64 (8 bytes)
   - script_pubkey_len: u64 (8 bytes)
   - script_pubkey: [script_pubkey_len] bytes
6. lock_time: u32 (4 bytes)

### 해시
- txid_v2 = SHA256(SHA256(canonical_bytes_v2)) (32 bytes)

### 확장/버전 전략
- 향후 필드 추가 시 규격 시작 부분에 1바이트 버전 태그를 도입하여 하위 호환성을 유지하는 방안을 권장한다.

## 역호환성(Backwards compatibility)

- v1(JSON) 기반 `txid_v1`는 레거시로 유지한다.
- 업그레이드 초기 단계에서 노드는 *동시에* `txid_v1`과 `txid_v2`를 계산·저장하고 로그로 불일치(있다면)를 보고해야 한다.
- 마이그레이션 기간 동안 블록 생성/전파는 v2를 우선 사용하되, 네트워크 레벨의 호환성 표시는 별도의 프로토콜 논의가 필요하다.

## 마이그레이션 계획

1. **계산 일관성 검증 단계**: 새 노드는 둘 다 계산해 내부 일관성 검증을 1주 이상(또는 더 긴 테스트 노드 운영 기간) 수행한다.
2. **벡터 수집/분석**: 교차-언어 벡터(샘플 세트)를 수집하여 불일치율을 측정한다.
3. **재인덱싱 도구 제공**: 기존 디스크형 UTXO 인덱스를 v2키로 변환하는 `utxo-reindex`를 제공한다.
4. **점진적 활성화**: 충분한 관찰 기간 후 v2를 기본으로 전환하고, 네트워크 메시지 포맷에 `txid_version` 필드를 도입하는 절차를 밟는다.

## 테스트 케이스 및 수용 기준 (Acceptance criteria)

- 단위테스트: `canonical_bytes_v2()`가 알려진 입력에 대해 기대 바이트열을 반환한다.
- 통합테스트: `txid_v1`/`txid_v2` 불일치 케이스가 발견되면 그 원인을 문서화한다.
- 교차언어 검증: Python/Go 예제 구현으로 동일한 바이트열과 txid를 생성할 수 있어야 한다. (현재 `tools/verify_vectors.py` 참조)
- CI: `crates/tenebrium-utxo/examples/generate_vectors.rs`가 생성한 `generated_vectors.json`을 기준으로 CI가 비교, 불일치 시 자동 PR 생성

## 벤치마크 목표

- `canonical_bytes_v2()` + double-SHA256의 평균 처리 시간이 v1(JSONserialize + sha)보다 유의미하게 낮아야 함(측정 필요).
- 벤치마크 스크립트를 작성해 CI 또는 별도 벤치마크 워크플로로 실행 권장.

## 보안 고려사항

- 입력 경계(스크립트 길이, vin/vout 개수)는 crate 상수(`MAX_SCRIPT_SIZE`, `MAX_TX_INOUTS`)에서 제한한다.
- 길이 접두사는 파서 공격(예: 길이표기 불일치)으로부터 보호하나, 파서 구현은 길이 착오 및 오버플로 검사를 반드시 수행해야 한다.

## 구현 참고 (코드 매핑)

- 참조 구현: `crates/tenebrium-utxo/src/lib.rs`
  - `Transaction::canonical_bytes_v2()`
  - `Transaction::txid_v2()`
  - `Transaction::make_outpoints_v2()`
- 테스트 벡터: `crates/tenebrium-utxo/test_vectors/vectors.json`
- 생성기: `crates/tenebrium-utxo/examples/generate_vectors.rs`
- 교차검증: `tools/verify_vectors.py`

## 개방 이슈

1. 네트워크 메시지 레벨에서 `txid_version`을 어떻게 도입할지(메시지 호환성 포함) 결정 필요
2. 재인덱싱 도구의 안정성·성능 목표 설정 필요
3. 최종 RFC를 승인할 시기 및 릴리스 타이밍(예: v0.2 기준선)

## 참고 문헌

- 내부 초안: `crates/tenebrium-utxo/CANONICAL_TXID_V2.md`
- 테스트 벡터 생성기 및 검증 스크립트 (위 참조 경로)

---

*작성 완료 — 리뷰 및 의견 제시 바랍니다.*
