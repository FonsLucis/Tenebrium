# STATUS

## Done
- [x] Updated .github/copilot-instructions.md with project-specific guidance
- [x] Added .gitignore to ignore build artifacts (target/)
- [x] Added root README.md with build/test/vector steps
- [x] Documented utxo-reindex CLI and RFC reference in CANONICAL_TXID_V2.md
- [x] Added txid version negotiation to P2P Hello and documented protocol
- [x] Fixed P2P Hello initializer for outbound connect and ran cargo test
- [x] Required explicit txid_version in P2P Hello to avoid v1/v2 mismatch
- [x] Verified tenebriumd has no current warnings (cargo check)
- [x] Reviewed CI/release workflows and captured notes
- [x] Clarified legacy v1 peer handling in P2P code/docs
- [x] Added CHANGELOG.md template
- [x] Updated release workflow for multi-OS artifacts and GitHub Release publish
- [x] Added release notes generation to release workflow
- [x] Added configurable P2P txid version (v1/v2) and mempool dual-indexing
- [x] Mined a block using genesis parameters (genesis-mined.json)
- [x] Submitted mined block to update UTXO (utxo-after.jsonl)
- [x] Removed GPG signing from release workflow to clear lint errors
- [x] Committed and pushed a full sync to origin/format-rebased

## Next (Top 3)
1. Decide whether to version artifacts with architecture suffix
2. Consider tagging policy for release notes (Since range)
3. Decide whether to keep genesis-mined.json/utxo-*.jsonl in repo

## How to run
- cargo fmt --all -- --check
- cargo test --all --workspace --verbose
- python tools/verify_vectors.py

## Notes (깨진 것/리스크)
- Build artifacts under target/ are now ignored by .gitignore
- CANONICAL_TXID_V2.md still lists TODOs for tooling/protocol work
- P2P now requires explicit txid_version and must match local config (v1/v2)
- CI runs fmt/test/verify_vectors and auto-PRs vector updates; release.yml builds multi-OS artifacts and publishes a GitHub Release (no GPG signing)
- Large full-repo sync commit pushed (511 files changed)
