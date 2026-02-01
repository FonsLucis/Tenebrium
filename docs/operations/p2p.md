# P2P 운영 가이드

## 실행 예시
- 기본 실행
  - `tenebriumd p2p --listen 0.0.0.0:8333 --network mainnet`
- 피어 지정
  - `tenebriumd p2p --peer 1.2.3.4:8333 --peer 5.6.7.8:8333`
- 시드 파일 지정
  - `tenebriumd p2p --seed-file ./seeds.txt`
- 상태 로그 주기 출력
  - `tenebriumd p2p --stats-interval 30`
- 로그 레벨/로그 파일
  - `tenebriumd p2p --log-level info --log-file /path/to/tenebriumd.log`

## 보호 기능(요약)
- 메시지 크기 제한, 리스트 길이 제한
- 피어 상한, 임시 밴(BAN_DURATION)
- 레이트 리밋(메시지/분)
- read/write 타임아웃
- 시드 재시도 백오프(지수 증가, 최대 60초)
- 시드 폴백: 주기적으로 다른 시드에 연결 시도

## 로그 레벨
- `error`, `warn`, `info`, `debug`

## 주의
- `--stats-interval`은 0이면 비활성화됩니다.
- 피어가 비정상 메시지를 보내면 차단될 수 있습니다.
 - 시드 파일은 한 줄에 하나의 주소를 넣고, `#`로 주석 처리가 가능합니다.
