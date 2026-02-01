# DB 백업/복구

## 백업
- `tenebriumd db-backup --data-dir <data_dir> --out-dir <backup_dir>`
- 기존 백업이 있으면 실패합니다. 덮어쓰려면 `--force` 사용.

## 복구
- `tenebriumd db-restore --backup-dir <backup_dir> --data-dir <data_dir>`
- 대상 DB가 있으면 실패합니다. 덮어쓰려면 `--force` 사용.

## 권장 플로우
1) `db-backup`으로 스냅샷 생성
2) `db-migrate --dry-run --json`로 검증
3) 필요 시 `db-migrate` 수행
4) 문제 발생 시 `db-restore`로 복구
