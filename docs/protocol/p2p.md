# Tenebrium P2P 프로토콜 (요약)

## 전송 형식
- 메시지 프레이밍: 4바이트 big-endian 길이 + JSON 바디
- 최대 메시지 크기: 10MB

## 메시지 타입
- `Hello { version, network, node_id, txid_version }`
- `Addr(Vec<String>)`
- `Inv { txids, blocks }`
- `GetTx(Vec<[u8;32]>)`
- `GetBlock(Vec<[u8;32]>)`
- `GetHeaders { locator }`
- `Headers(Vec<BlockHeader>)`
- `Ping` / `Pong`
- `Tx(Transaction)`
- `Block(Block)`

## 제한(기본값)
- Addr: 최대 1,000
- Inv: txids/blocks 각각 최대 5,000
- GetTx/GetBlock/GetHeaders: 최대 2,000
- Headers: 최대 2,000
- node_id 최대 길이 64, network 최대 길이 16

## 기본 검증
- 프로토콜 버전 범위
- 네트워크 ID 일치
- txid 버전 일치(현재 v2만 지원)
- 메시지 크기/리스트 길이 제한
- 헤더 규칙(시간/난이도/PoW)
- 블록 검증(머클 루트, 코인베이스 규칙, 수수료/보상)

## 피어 정책
- 피어 수 제한, 주소 목록 크기 제한
- 메시지 레이트 리미트
- 비정상 메시지는 일정 시간 밴 처리

## txid 버전
- `Inv`/`GetTx`의 txid는 `Hello.txid_version`에 따라 v1/v2 중 하나를 사용합니다.
- `Hello.txid_version`은 필수이며, 로컬 설정과 일치해야 합니다.
- 로컬 설정은 `tenebriumd p2p --txid-version v1|v2`로 지정합니다.
- `txid_version`이 누락되거나 불일치하면 즉시 거절합니다.

## 레거시 v1 피어 호환
- v1(txid_version=1) 피어는 `--txid-version v1` 설정으로만 수용됩니다.
- v1/v2는 별도 네트워크(예: `mainnet-v1`, `mainnet`)로 분리 운영을 권장합니다.
