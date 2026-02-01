# 체크섬/서명 가이드

## 체크섬 생성
- SHA256 체크섬을 사용
- 출력 파일: `SHA256SUMS`

## 서명
- `SHA256SUMS`와 바이너리 파일을 서명
- 서명 파일: `SHA256SUMS.sig`, `<binary>.sig`
 - 자동화: `tools/build_release.ps1 -Sign [-GpgKey <key>]`

## 검증
- 체크섬 검증 후 서명 검증을 수행

## 배포
- 릴리스 페이지에 바이너리/체크섬/서명 함께 업로드
