# Mainnet 출시 체크리스트

**목적:** 메인넷(프로덕션) 론칭을 안전하고 재현 가능하게 수행하기 위한 최소 작업 항목과 명령 모음입니다. 담당자, 완료 조건, 관련 파일/명령을 빠르게 참조할 수 있도록 구성했습니다.

## 범위
- 코드베이스: `crates/*`, CI, 벡터 생성/검증, 배포 아티팩트
- 제외: 법적/마케팅/재무 의사결정(별도 프로세스)

---

## 1. 설계·정책 확정
- [ ] 제네시스 블록 명세 및 RFC 작성 (`docs/rfcs/`)
  - 파일 예: `docs/rfcs/0001-canonical-txid-v2.md` 스타일
- [ ] 컨센서스 파라미터 고정 (`crates/tenebrium-consensus`)
- [ ] 네트워크 규칙(포트, 메시지, 시드 노드) 문서화
  - 관련 문서: `docs/consensus/params.md`, `docs/protocol/p2p.md`

## 2. 코드 품질·검증
- [ ] 포맷/테스트 통과
  - `cargo fmt --all -- --check`
  - `cargo test --all --workspace --verbose`
- [ ] 직렬화/txid 벡터 재생성 + 검증
  - `cargo run -p tenebrium-utxo --example generate_vectors > crates/tenebrium-utxo/test_vectors/generated_vectors.json`
  - `python tools/verify_vectors.py` (또는 CI와 동일한 비교 로직)
  - 벡터가 변경되면 PR 생성 규칙을 준수(자동 PR 절차는 `.github/workflows/ci.yml` 참조)
- [ ] 추가: 통합 테스트, 퍼즈(특히 consensus/serialization 경계)
  - 퍼징 가이드: `docs/security/fuzzing.md`

## 3. 보안·감사
- [ ] 외부 보안 감사(컨센서스, 네트워크, 암호화 우선)
- [ ] 버그바운티 프로세스 및 긴급 패치 플레이북
- [ ] 종속성 스캔(취약점, 라이선스)
  - 내부 체크리스트: `docs/security/audit-checklist.md`

## 4. 테스트넷 → 스테이지 → 메인넷 절차
- [ ] Private devnet 시작(설정 템플릿 제공)
- [ ] Public testnet(수 주)으로 확장: 운영 문서와 모니터링 검증
- [ ] 장기간 스테이지(수주~수개월) 후 메인넷 전환
- 배포 예시: `cargo build --release -p tenebriumd` → 서명된 바이너리 배포

## 5. 릴리스 아티팩트 & 재현성
- [ ] 서명된 바이너리, 체크섬, 빌드 스크립트(정확한 toolchain 지정)
- [ ] 재현 가능한 빌드 문서(환경, rustup toolchain, OS 이미지)
- [ ] 릴리스 노트 템플릿(변경점, 마이그레이션 가이드, 알려진 이슈)
  - 릴리스 문서: `docs/release/release-process.md`, `docs/release/checksums.md`

## 6. 운영·모니터링
- [ ] 노드 헬스 체크(블록 생성/동기화 상태, 메모리, CPU)
- [ ] 로그/메트릭 수집(RPC 오류율, 블록 간격, 포크 이벤트)
- [ ] 알림/오너십(담당자, 연락망, 운영시간 SLA)

## 7. 롤백 및 비상 대응
- [ ] 구버전으로의 안전한 롤백 절차(체인 포크 위험 검토)
- [ ] 블록체인 분기시 공지·시나리오별 행동지침
- [ ] 긴급 공지 템플릿(노드 운영자/사용자 대상)

## 8. 배포 후 검증 체크리스트
- [ ] 초기 블록 타깃(예: 평균 600s) 준수 확인
- [ ] 여러 독립 노드에서 블록 검증 성공
- [ ] tx 처리 및 RPC 응답 정상
- [ ] 벡터/직렬화 동작 재검증

---

## Sign-off 템플릿 (PR/릴리스 본문)
- TASK: 메인넷 론칭 – <간단 설명>
- PLAN:
  1. 제네시스 RFC 완성
  2. 테스트넷(2주) 검증
  3. 보안감사/버그바운티(기간) 완료
  4. 릴리스 준비(바이너리 서명, 체크섬, 릴리스 노트)
- CHANGED FILES:
  - (예) `docs/rfcs/*`, `crates/tenebrium-consensus/*`, `crates/tenebriumd/*`
- HOW TO RUN:
  - `cargo build --release -p tenebriumd`
  - `python tools/verify_vectors.py`
- RISKS / NEXT:
  - (예) 긴급 롤백 절차 미비 → 우선순위: 문서화 및 테스트

---

피드백: 필요한 항목(예: 자동화 스크립트, 배포용 IaC 템플릿, 테스팅 도커 이미지)을 알려주시면 체크리스트에 추가하고 관련 예제 파일을 생성해 드립니다.