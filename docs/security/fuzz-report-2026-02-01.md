# 퍼징 리포트

## 실행 정보
- 날짜: 2026-02-01
- 실행자: GitHub Copilot
- 대상: tx_json, block_json, header_json, tx_json_strict, p2p_message_json, utxo_kv
- 커맨드:
  - `cargo +nightly fuzz run tx_json -- -runs=1000`
  - `cargo +nightly fuzz run block_json -- -runs=1000`
  - `cargo +nightly fuzz run header_json -- -runs=1000`
  - `cargo +nightly fuzz run tx_json_strict -- -runs=1000`
  - `cargo +nightly fuzz run p2p_message_json -- -runs=1000`
  - `cargo +nightly fuzz run utxo_kv -- -runs=1000`
- 실행 시간: 단기 러닝(각 1000 runs)
- 환경: Windows, nightly toolchain

## 결과 요약
- 크래시 수: 0 (utxo_kv 제외)
- 재현 케이스 경로: 없음
- 재현 가능 여부: 해당 없음

## 주요 이슈
- utxo_kv: STATUS_DLL_NOT_FOUND (0xc0000135). 런타임 DLL 누락으로 실행 실패.

## 조치
- 수정 PR: N/A
- 재검증 결과: VC++ 런타임 설치 후에도 실패 → 재부팅/런타임 재설치 필요
