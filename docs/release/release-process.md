# 릴리스 프로세스 (메인넷)

## 목표
- 재현 가능한 릴리스 아티팩트 생성
- 바이너리 서명 및 체크섬 배포

## 단계
1) 툴체인 고정
- rustup toolchain pin (예: stable-1.xx.x)

2) 빌드
- `cargo build --release -p tenebriumd`
- `cargo build --release -p tenebrium-cli`
 - 자동화 스크립트: `tools/build_release.ps1`

3) 체크섬 생성
- SHA256 체크섬 생성 및 배포

4) 서명
- 릴리스 바이너리와 체크섬 파일 서명

5) 검증
- 다른 환경에서 체크섬 검증
- 제네시스 해시/파라미터 재확인

6) 배포
- 릴리스 노트 작성
- 바이너리/체크섬/서명 업로드

## 자동화 스크립트
- 빌드/체크섬/서명: `tools/build_release.ps1`
- 태그 생성: `tools/create_release_tag.ps1`
- 릴리스 노트 템플릿: `docs/release/release-notes-template.md`
- 릴리스 노트 생성: `tools/generate_release_notes.ps1`

## CI 릴리스
- 워크플로: `.github/workflows/release.yml`
- 옵션 서명: `GPG_PRIVATE_KEY`, `GPG_PASSPHRASE` 시크릿 설정 시 자동 서명

## 주의
- 릴리스 전후로 제네시스/컨센서스 파라미터 변경 금지
