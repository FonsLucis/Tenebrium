# Tenebrium 운영 가이드

## P2P 노드 실행
- 기본 실행: `tenebriumd p2p --listen 0.0.0.0:8333`
- 피어 추가: `--peer <host:port>` (복수 가능)
- 네트워크: `--network mainnet|testnet|devnet`
- PoW 검증 끄기: `--no-pow-check`
- 데이터 디렉터리: `--data-dir <path>` (sled DB/utxo.jsonl 저장)
- 상태 출력 주기: `--stats-interval <seconds>` (0이면 비활성)
- 로그 레벨: `--log-level error|warn|info|debug`
- 로그 파일: `--log-file <path>`

### 상태 출력 포맷
`[stats] peers=<n> mempool=<n> mempool_bytes=<n> utxo=<n> tip=<hash> height=<h>`

## DB 마이그레이션
- 드라이런: `tenebriumd db-migrate --data-dir <path> --target <ver> --dry-run`
- JSON 요약: `--json` (드라이런에서만)
- 백업 포함: `--backup`

## DB 백업/복구
- 백업: `tenebriumd db-backup --data-dir <path> --out-dir <path>`
- 복구: `tenebriumd db-restore --backup-dir <path> --data-dir <path>`
- 덮어쓰기: `--force`

## 유의사항
- 드라이런은 쓰기 없이 검증만 수행합니다.
- P2P는 기본적인 검증만 수행하므로 운영 전 추가 검증/모니터링을 권장합니다.
