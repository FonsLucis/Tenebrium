#!/usr/bin/env python3
"""Verify tenebrium-utxo test vectors (v1 and v2 canonical bytes and txid).

Usage: python tools/verify_vectors.py
Exits with code 0 if all vectors match, non-zero otherwise.
"""
import sys
import json
import hashlib
from collections import OrderedDict
from pathlib import Path

VECTORS_PATH = Path("crates/tenebrium-utxo/test_vectors/vectors.json")


def double_sha256(b: bytes) -> bytes:
    return hashlib.sha256(hashlib.sha256(b).digest()).digest()


def canonical_bytes_v1_from_txdict(tx: dict) -> bytes:
    # Reconstruct JSON in field order: version, vin, vout, lock_time
    def prevout_order(po):
        return OrderedDict([("txid", po["txid"]), ("vout", po["vout"])])

    def vin_entry(vin):
        return OrderedDict([
            ("prevout", prevout_order(vin["prevout"])),
            ("script_sig", vin["script_sig"]),
            ("sequence", vin["sequence"]),
        ])

    def vout_entry(vout):
        return OrderedDict([("value", vout["value"]), ("script_pubkey", vout["script_pubkey"])])

    od = OrderedDict()
    od["version"] = tx["version"]
    od["vin"] = [vin_entry(v) for v in tx["vin"]]
    od["vout"] = [vout_entry(v) for v in tx["vout"]]
    od["lock_time"] = tx["lock_time"]
    # Use separators to match Rust serde_json::to_vec formatting (no spaces)
    s = json.dumps(od, separators=(",", ":"), ensure_ascii=False)
    return s.encode("utf-8")


def canonical_bytes_v2_from_txdict(tx: dict) -> bytes:
    # Build binary according to spec (little-endian ints, u64 lengths)
    out = bytearray()
    # version i32
    out.extend((tx["version"] & 0xFFFFFFFF).to_bytes(4, "little", signed=True))
    # vin_count u64
    out.extend((len(tx["vin"])).to_bytes(8, "little"))
    for vin in tx["vin"]:
        prev = vin["prevout"]
        # prevout.txid: 32 bytes
        out.extend(bytes(prev["txid"]))
        # prevout.vout: u32
        out.extend((prev["vout"] & 0xFFFFFFFF).to_bytes(4, "little"))
        # script_sig_len: u64
        out.extend((len(vin["script_sig"]) ).to_bytes(8, "little"))
        # script_sig bytes
        out.extend(bytes(vin["script_sig"]))
        # sequence u32
        out.extend((vin["sequence"] & 0xFFFFFFFF).to_bytes(4, "little"))
    # vout_count u64
    out.extend((len(tx["vout"]) ).to_bytes(8, "little"))
    for vout in tx["vout"]:
        # value u64
        out.extend((vout["value" ] & 0xFFFFFFFFFFFFFFFF).to_bytes(8, "little"))
        # script_pubkey_len u64
        out.extend((len(vout["script_pubkey"]) ).to_bytes(8, "little"))
        # script_pubkey bytes
        out.extend(bytes(vout["script_pubkey"]))
    # lock_time u32
    out.extend((tx["lock_time"] & 0xFFFFFFFF).to_bytes(4, "little"))
    return bytes(out)


def hex_of(b: bytes) -> str:
    return b.hex()


def run():
    if not VECTORS_PATH.exists():
        print(f"Vectors file not found: {VECTORS_PATH}")
        return 2
    raw = VECTORS_PATH.read_text(encoding="utf-8-sig")
    vecs = json.loads(raw)
    failures = 0
    for v in vecs:
        name = v.get("name")
        tx = v["tx"]
        print(f"Checking vector: {name}")
        # v2
        c2 = canonical_bytes_v2_from_txdict(tx)
        txid2 = double_sha256(c2)
        if hex_of(c2) != v["canonical_v2"]:
            print(f"  canonical_v2 mismatch for {name}: expected {v['canonical_v2']}, got {hex_of(c2)}")
            failures += 1
        if hex_of(txid2) != v["txid_v2"]:
            print(f"  txid_v2 mismatch for {name}: expected {v['txid_v2']}, got {hex_of(txid2)}")
            failures += 1
        # v1
        c1 = canonical_bytes_v1_from_txdict(tx)
        txid1 = double_sha256(c1)
        if hex_of(c1) != v["canonical_v1"]:
            print(f"  canonical_v1 mismatch for {name}: expected {v['canonical_v1']}, got {hex_of(c1)}")
            failures += 1
        if hex_of(txid1) != v["txid_v1"]:
            print(f"  txid_v1 mismatch for {name}: expected {v['txid_v1']}, got {hex_of(txid1)}")
            failures += 1
    if failures == 0:
        print("All vectors match ✔️")
        return 0
    else:
        print(f"{failures} mismatches found ❌")
        return 1


if __name__ == "__main__":
    sys.exit(run())
