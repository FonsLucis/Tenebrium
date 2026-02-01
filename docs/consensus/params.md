# Consensus 파라미터

이 문서는 현재 코드에 반영된 컨센서스·경제 파라미터를 정리합니다.

## 블록/난이도
- 목표 블록 시간: 600s
- 난이도 윈도우: 10 blocks
- 초기 bits: 0x207fffff
- 허용 미래 시간 오차: 2h

## 보상
- 초기 보상: 50 * 10^8 (사토시 단위)
- 할빙 주기: 210,000 blocks

## 확인 위치
- [crates/tenebriumd/src/p2p.rs](crates/tenebriumd/src/p2p.rs) 상단 상수들

## 제네시스
- genesis.json: [docs/consensus/genesis.json](docs/consensus/genesis.json)
- time: 1769936400 (2026-02-01T00:00:00Z)
- bits: 0x207fffff
- nonce: 2
- merkle_root: a979027b27f1d8c224c9baed9d5d19e49b44aee40308a824f5d03aad12cdb33a
- block hash: 67187f8b304a9e54002ad4befec39f1f0203d7b5cabebdc956d5e2d602ead46f
- coinbase script_pubkey: 54656e65627269756d

## 메인넷 고정 절차
1) 파라미터 최종 값 확정
2) 제네시스 블록 생성
3) RFC 갱신 및 버전 태깅
