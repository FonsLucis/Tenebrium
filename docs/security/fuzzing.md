# 퍼징 가이드

## 목적
- 입력 파서/직렬화 경계에서 패닉·오버플로·비정상 동작 탐지

## 준비
- `cargo install cargo-fuzz`

## 실행
- `cargo fuzz run tx_json`
- `cargo fuzz run block_json`
- `cargo fuzz run header_json`
- `cargo fuzz run tx_json_strict`
- `cargo fuzz run p2p_message_json`
- `cargo fuzz run utxo_kv`

## 대상
- tx_json: Transaction JSON 파싱/검증/직렬화 경로
- block_json: Block JSON 파싱 및 머클 계산 경로
- header_json: BlockHeader JSON 파싱 및 PoW 검증 경로
- tx_json_strict: Transaction::from_json_bytes 경로
- p2p_message_json: P2P 메시지 JSON 파싱/검증 경로
- utxo_kv: UTXO KV 디코딩 경로

## 참고
- 크래시 발생 시 `fuzz/artifacts/`에 재현 입력이 저장됩니다.
- 릴리스 직전 장시간 퍼징을 권장합니다.
 - 리포트 템플릿: `docs/security/fuzz-report-template.md`
 - Windows에서 `STATUS_DLL_NOT_FOUND`가 발생하면
	 - Visual C++ Redistributable(VC++ 런타임) 설치
	 - 또는 동일 도구체인/환경에서 실행
