# RFC 0002 — UTXO Reindex (v1 → v2)

**상태:** Draft  
**작성일:** 2026-02-01  
**대상:** `tenebrium-utxo`, `tenebriumd`

---

## 1. 목적
기존 v1(JSON 기반) txid를 키로 사용하는 UTXO 인덱스를 **v2(binary canonical) txid** 기반으로 안전하게 재구성한다.

### 목표
- **정확성:** 동일한 UTXO 집합이 v2 키로 재매핑되어야 함
- **재현성:** 동일한 입력에서 동일한 결과
- **복구성:** 중단 시 안전한 재시작 및 롤백
- **가시성:** 진행률, 통계, 오류 요약 제공

### 비목표
- 체인 규칙 변경
- 네트워크 프로토콜 변경
- 데이터베이스 엔진 교체

---

## 2. 용어
- **v1 txid:** `canonical_bytes_v1`의 double-SHA256
- **v2 txid:** `canonical_bytes_v2`의 double-SHA256
- **OutPoint:** (txid, vout)

---

## 3. 입력/출력

### 입력
- 기존 UTXO DB (v1 txid 키)
- 트랜잭션 원본(블록/tx 스토어)
- 체인 파라미터(검증 규칙은 동일)

### 출력
- 새로운 UTXO DB (v2 txid 키)
- 재인덱싱 리포트(JSON)

---

## 4. 동작 개요
1. UTXO DB를 **읽기 전용**으로 오픈
2. 블록/트랜잭션 저장소를 순회하며 txid_v2 계산
3. 각 UTXO를 v2 OutPoint로 변환하여 신규 DB에 기록
4. 통계 및 오류를 리포트로 기록
5. 검증 단계(랜덤 샘플 및 전체 카운트 비교)

---

## 5. 에러 처리 원칙
- `unwrap()`/`expect()` 금지(동일 코드베이스 규칙 준수)
- 도메인 에러 타입 정의: `ReindexError`
- 복구 가능한 오류는 **건너뛰기 + 보고서 기록**
- 치명적 오류는 즉시 중단(데이터 손상 위험)

---

## 6. 데이터 포맷

### 리포트 (JSON)
```json
{
  "started_at": "2026-02-01T00:00:00Z",
  "finished_at": "2026-02-01T01:00:00Z",
  "total_inputs": 123456,
  "total_outputs": 123450,
  "skipped": 6,
  "errors": [
    {"kind": "MissingTx", "txid_v1": "..."}
  ]
}
```

---

## 7. CLI 스펙(예시)
```
tenebriumd utxo-reindex \
  --db ./data/utxo_v1 \
  --out ./data/utxo_v2 \
  --report ./data/reindex_report.json \
  --verify
```

옵션:
- `--verify`: 재인덱싱 후 샘플/카운트 검증 수행
- `--resume`: 중단된 진행을 체크포인트에서 재개
- `--dry-run`: DB 쓰기 없이 검증만 수행

---

## 8. 검증 단계
- 전체 UTXO 개수 비교
- 랜덤 샘플 n개 검증
- `txid_v1`/`txid_v2` 매핑 충돌 탐지

---

## 9. 보안 고려사항
- 입력 데이터 검증(스크립트 길이 제한, 인풋/아웃풋 개수 제한)
- 재인덱싱 중 시스템 장애 시 원본 DB 손상 방지(읽기 전용)

---

## 10. 구현 위치 제안
- `crates/tenebrium-utxo`: 변환 로직/검증 유틸
- `crates/tenebriumd`: CLI 및 저장소 IO

---

## 11. 후속 작업
- POC 구현
- CI 내 스모크 테스트 추가
- 문서화 및 운영 가이드
