# RFC-0004: Genesis Block & Mainnet Params

## 상태
- Draft

## 목적
메인넷 출시를 위한 제네시스 블록과 핵심 파라미터를 고정한다.

## 배경
현재 컨센서스/네트워크 파라미터는 코드 상수로 정의되어 있으며, 메인넷 출시를 위해 공식 명세가 필요하다.

## 메인넷 파라미터(제안)
- 목표 블록 시간: 600s
- 난이도 윈도우: 10 blocks
- 초기 bits: 0x207fffff
- 허용 미래 시간 오차: 2h
- 초기 보상: 50 * 10^8
- 할빙 주기: 210,000
- 네트워크 ID: mainnet
- 기본 포트: 8333

## 제네시스 블록
- version: 1
- prev_block_hash: 000..000
- time: 1769936400 (2026-02-01T00:00:00Z)
- bits: 0x207fffff
- nonce: 2
- coinbase script_pubkey: 54656e65627269756d
- merkle_root: a979027b27f1d8c224c9baed9d5d19e49b44aee40308a824f5d03aad12cdb33a
- block hash: 67187f8b304a9e54002ad4befec39f1f0203d7b5cabebdc956d5e2d602ead46f

## 생성 절차(예시)
1) 코인베이스 트랜잭션 확정(스크립트/수령 주소)
2) `tenebriumd mine`으로 제네시스 블록 생성
3) 생성된 블록 해시/머클 루트/nonce 기록
4) 문서/코드에 고정 값 반영

## 합의 고정 항목
- 위 파라미터 및 제네시스 헤더 값은 **메인넷 출시 이후 변경 불가**

## 체크리스트 연동
- [docs/release/mainnet-checklist.md](docs/release/mainnet-checklist.md)
