---
agent: agent
---
Define the task to achieve, including specific requirements, constraints, and success criteria.

# Tenebrium Agent Instructions (Rust / Security-First)
- Tenebrium 프로젝트의 Rust 기반 코인 개발을 위한 에이전트 지침입니다.

## Operating mode
- Output language: 한국어만. (코드/식별자/경로/에러 메시지는 예외)

## Rust standards
- Rust stable, Edition 2021.
- consensus/파서/네트워크 입력 영역: `unwrap/expect` 금지, `Result` 기반 처리.
- 입력 검증(길이/범위/형식) 필수. 에러 타입은 명확히.
- 작은 모듈, 명확한 경계. 순환 의존 금지.
- 외부 크립토는 "직접 구현 금지", 검증된 크레이트 사용.


## Tenebrium v0.1 baseline (can evolve)
- L1: PoW + UTXO, 가치저장 우선(보수적으로)
- Target block time: 600s
- PoW hash: SHA-256d (v0.1)
- L2 우선(채널/정산), L1은 안정성 최우선
- Post-quantum: 하이브리드 서명(클래식 + PQ) 방향, 단 v0.1에서는 인터페이스부터 고정

## Deliverables format
- 작업 시작 시:
  - TASK: 목표 1줄
  - PLAN: 단계 3~7개
- 작업 종료 시:
  - CHANGED FILES: 수정/생성 파일 목록
  - HOW TO RUN: 실행/테스트 명령어
  - RISKS: 남은 리스크/다음 우선순위 3~7개

  ## Hard constraints
- 존재하지 않는 파일/구조를 가정하지 않는다.
- 설명보다 결과물을 우선한다.
- 보안/무결성에 의심이 있으면 진행 중단하고 근거를 제시한다.