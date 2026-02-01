# RFC 0003 — Wallet File Format (v1)

- 상태: Draft
- 작성일: 2026-02-01
- 관련: docs/wallet/wallet-file-format.md

## 1. 목표
- 지갑 파일의 암호화/복호화 방식과 필드를 표준화한다.
- 구현 간 호환성을 확보한다.
- 향후 버전업 전략을 명시한다.

## 2. 비목표
- 키 파생(HD wallet) 스펙 정의
- 주소 체계 변경
- 네트워크/전송 프로토콜 정의

## 3. 용어
- Secret key: ed25519 32바이트 개인키
- Address: Bech32(HRP `tn`) + SHA-256(pubkey)

## 4. 암호화 스펙
- KDF: scrypt
- Cipher: AES-256-GCM
- Nonce: 12 bytes
- Salt: 16 bytes
- KDF 파라미터: N=32768, r=8, p=1, output length=32

## 5. JSON 포맷
```json
{
  "version": 1,
  "kdf": "scrypt",
  "kdf_params": {
    "salt_hex": "<hex>",
    "n": 32768,
    "r": 8,
    "p": 1
  },
  "cipher": "aes-256-gcm",
  "nonce_hex": "<12-byte hex>",
  "ciphertext_hex": "<hex>",
  "public_key_hex": "<32-byte hex>",
  "address": "tn1..."
}
```

## 6. 유효성 규칙
- `version` 필수. 현재 1만 지원.
- `kdf`는 `scrypt`만 지원.
- `cipher`는 `aes-256-gcm`만 지원.
- `kdf_params.n`는 2의 거듭제곱이어야 한다.
- 복호화 후 secret key 길이는 32바이트여야 한다.
- 복호화된 secret key로 재계산한 `public_key_hex`, `address`는 파일 값과 일치해야 한다.

## 7. 백업/재암호화
- 기존 passphrase로 복호화 후 새 passphrase로 재암호화하여 저장한다.
- 포맷은 동일하며, `salt_hex`와 `nonce_hex`는 매번 새로 생성한다.

## 8. 버전 전략
- v1 내에서는 **하위 호환 추가 필드**만 허용.
- 호환 불가 변경은 `version`을 올리고 로더에서 명시적으로 지원한다.

## 9. 구현 참조
- tenebrium-core: wallet 파일 생성/복호화
- tenebrium-cli: wallet save/load/backup

## 10. 테스트 벡터
파일: `docs/wallet/wallet-vectors.json`

입력:
- secret_hex: `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f`
- passphrase: `correct horse battery staple`
- salt_hex: `000102030405060708090a0b0c0d0e0f`
- nonce_hex: `0f0e0d0c0b0a090807060504`
- n=32768, r=8, p=1

출력(요약):
```json
{
  "ciphertext_hex": "3290608a78fc45997c4643fe701910c441417ba80db5770c43a245df0203b3dac1218a6869936934da79f5e93c4e3ee1",
  "public_key_hex": "03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8",
  "address": "tn12er44f65vdr5cq59mawm7272ku76v5f43qu7ndm5sxew4vg8wzxqdjnd7h"
}
```
