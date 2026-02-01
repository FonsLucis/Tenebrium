# Tenebrium UTXO — Canonical TXID v2 규격 (초안) 🚀

## 목적 및 범위
- 본 문서는 Tenebrium **v0.2**에서 도입한 **Canonical TXID v2** 직렬화 규격의 설계 의도, 이진 레이아웃, 검증 방법 및 마이그레이션/호환성 고려사항을 정리합니다.
- v1(JSON 기반)과의 차이점 및 호환성 전략을 명시합니다.

---

## 요약 (핵심 사항) ✅
- Canonical TXID v2는 **deterministic 이진 직렬화**(binary encoding)를 사용합니다.
- 직렬화된 바이트의 double-SHA256 해시(= SHA256(SHA256(bytes)))가 txid입니다.
- 모든 정수는 **little-endian**으로 인코딩합니다.
- 가변길이 바이트(스크립트)는 **u64 길이 접두사** + 실제 바이트로 표현합니다.
- v1(JSON) 포맷은 유지하지만 기본 txid는 v2를 사용합니다(백워드 호환성 보장).

---

## 데이터 레이아웃 (정밀 규격)
- 전체 필드 순서 및 인코딩(모두 소문자 이름은 구현 필드 이름과 대응):
  1. version: i32 (4 bytes, little-endian)
  2. vin_count: u64 (8 bytes)
  3. for each vin (in order):
     - prevout.txid: 32 bytes (raw)
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

- 장점: 단순하고 빠른 파싱, 언어 독립적 구현 가능, 길이-접두사로 구조가 명확함.
- 제한 사항: 현재 스펙은 정수 리틀엔디안으로 고정되어 있으므로 교차-플랫폼 일관성이 보장됩니다.

---

## 왜 JSON(v1)에서 v2(바이너리)로 전환했나?
- JSON은 인간 읽기에는 편하지만, 네트워크/디스크 효율 및 해시 결정성 측면에서 불리합니다.
- 바이너리 직렬화는 명확한 바이트 레벨 표현을 제공하여 txid 충돌/불일치 가능성을 낮춥니다.
- 성능(직렬화/해시)과 크기(전송/저장) 측면에서 대규모 네트워크 운영에 적합합니다.

---

## 해시 함수
- txid = double_sha256(canonical_bytes_v2)
  - first = SHA256(canonical_bytes_v2)
  - second = SHA256(first)
  - txid = second (32 bytes)
- 이중 해시를 사용하여 기존 PoW/비트코인 계열과의 직관적 일관성을 유지합니다.

---

## 구현 노트 (현재 코드와의 대응)
- 구현 함수 목록(참고):
  - `Transaction::canonical_bytes_v2()` — 위 레이아웃을 생성
  - `Transaction::txid_v2()` — double-SHA256
  - `Transaction::make_outpoints_v2()` — txid + vout 인덱스로 OutPoint 생성
- 기존 `canonical_bytes_v1()` 및 `txid_v1()`는 **legacy**로 유지되어 마이그레이션을 지원합니다.

---

## 마이그레이션 / 호환성 전략
- 노드 업그레이드 시 권장 절차:
  1. 새 노드는 `txid_v1`과 `txid_v2` 모두를 계산하여 기존 UTXO/메모리 인덱스와 검증 상충이 없는지 점검합니다.
  2. 충분한 보수 기간(예: 릴리스 + 테스트넷 운영기간) 동안 두 txid를 함께 유지하고 로그/보고를 통해 불일치율을 관찰합니다n
  3. 문제 없을 경우, 업데이트 노드는 새로운 블록 생성에 `txid_v2`를 사용하며 네트워크 프로토콜(메시지 포맷)에 txid version 정보를 포함하도록 점진적 변경을 권장합니다.
- 기존 디스크형 UTXO 인덱스 재인덱싱 도구 제공 필요: v1 기반 인덱스를 v2 키로 재인덱스 하는 툴.

## UTXO 재인덱싱 도구
- CLI: `tenebriumd utxo-reindex` (구현: `crates/tenebriumd/src/main.rs`)
- 설계 문서: `docs/rfcs/0002-utxo-reindex.md`
- 예시:
```
tenebriumd utxo-reindex \
  --db ./data/utxo_v1 \
  --out ./data/utxo_v2 \
  --report ./data/reindex_report.json \
  --verify
```

---

## 테스트 및 검증
- 권장 테스트 케이스:
  - 단위 테스트: 다양한 트랜잭션(빈 vin/vout, 큰 스크립트, 최댓값 등)에 대해 `canonical_bytes_v2()` 결과 검증
  - 통합 테스트: `txid_v1` vs `txid_v2` 불일치 탐지 케이스
  - 교차 구현 테스트: 다른 언어(예: Python, Go)로 같은 입력에 대해 동일한 바이트/해시가 생성되는지 확인
  - 퍼즈 테스트: 가변 길이/경계값, 악성 입력(과도한 길이 등)으로 파서 안정성 검증

---

## 보안 및 공격 표면 고려사항
- 입력 길이는 이미 crate 레벨에서 제한(`MAX_SCRIPT_SIZE`, `MAX_TX_INOUTS`)을 적용함.
- canonical encoding은 길이-접두사 기반으로 애매모호성(예: 중첩 구조에서의 다양한 표현) 가능성을 제거합니다.
- 추후 포맷 확장(예: 새로운 필드 추가 시)은 버전 태그 추가(직렬화 시작에 u8 버전) 방식을 권장합니다.

---

## 성능 및 최적화
- 직렬화 비용: JSON -> 바이너리로 변경하면 바이트량과 CPU(파싱/해시) 비용이 감소합니다.
- 추천: 대량 처리 경로에서는 한번만 직렬화하여 버퍼 재사용, 또는 스트리밍 직렬화를 고려하세요.

---

## TODO(권장 작업 목록)
1. RFC 스타일의 공식 스펙(이 문서를 RFC 포맷으로 확장 및 저장소 최상위 `docs/`에 추가). ✅ (초안 작성 완료)
2. Cross-language test vectors 파일 생성 (JSON + hex bytes + expected txid). ✅
3. 재인덱싱 도구(`utxo-reindex`) 제작 (v1->v2 변환) 및 문서화. ✅
4. 네트워크 레벨의 txid 버전 표시 및 호환성 메시지 설계. 📡
5. 성능 벤치마크(직렬화+txid 해시) 및 필요시 바이너리 포맷 개선. ⚙️

---

## 부록: 예시
- 예시 트랜잭션 (pseudocode):
```
version = 1
vin_count = 1
vin[0]: prevout.txid = 32 zeros, prevout.vout = 0, script_sig = b"", sequence = 0
vout_count = 1
vout[0]: value = 50, script_pubkey = b"" 
lock_time = 0
```
- 이 트랜잭션의 `canonical_bytes_v2()`는 위 필드 순서대로 바이트를 이어붙인 바이트열이 됩니다. 해당 바이트열의 double-SHA256이 `txid_v2`입니다.

---

## 연락처 및 참고
- 구현 리포지토리: `crates/tenebrium-utxo`
- 이슈/토론: PR을 통해 제안하거나 `#spec` 채널에서 논의하세요.

---

감사합니다. 이 문서를 기반으로 공식 RFC 문서(포맷 문서) 및 test-vectors를 추가로 만들 수 있습니다.