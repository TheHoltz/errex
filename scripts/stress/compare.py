#!/usr/bin/env python3
"""Compare two stress-sweep result sets (e.g. before/after a perf change).

Usage:
  python3 compare.py results/ results-after/
"""
from __future__ import annotations

import json
import sys
from pathlib import Path


def load_set(d: Path) -> dict[str, dict]:
    out = {}
    for p in sorted(d.glob("*.json")):
        try:
            j = json.load(open(p))
        except Exception:
            continue
        out[j.get("label", p.stem)] = j
    return out


def fmt_ms(v) -> str:
    if v is None:
        return "—"
    if v < 1.0:
        return f"{v * 1000:.0f}µs"
    if v < 10.0:
        return f"{v:.2f}ms"
    return f"{v:.1f}ms"


def pct(before: float, after: float) -> str:
    if before == 0:
        return "—"
    delta = (after - before) / before * 100
    sign = "+" if delta >= 0 else ""
    return f"{sign}{delta:.0f}%"


def main(a: str, b: str) -> int:
    before = load_set(Path(a))
    after = load_set(Path(b))
    if not before or not after:
        print(f"need results in both {a} and {b}")
        return 1

    rows = []
    for label in sorted(set(before) | set(after)):
        b_run = before.get(label)
        a_run = after.get(label)
        if not b_run or not a_run:
            rows.append((label, "missing", "", "", "", "", "", "", ""))
            continue
        b_rps = b_run.get("achieved_rps", 0)
        a_rps = a_run.get("achieved_rps", 0)
        b_p99 = b_run.get("ingest_latency_ms", {}).get("p99")
        a_p99 = a_run.get("ingest_latency_ms", {}).get("p99")
        b_max = b_run.get("ingest_latency_ms", {}).get("max")
        a_max = a_run.get("ingest_latency_ms", {}).get("max")
        b_rss = b_run.get("daemon_rss_kb", {}).get("max", 0) / 1024
        a_rss = a_run.get("daemon_rss_kb", {}).get("max", 0) / 1024
        rows.append(
            (
                label,
                f"{b_rps:.0f}",
                f"{a_rps:.0f}",
                pct(b_rps, a_rps),
                fmt_ms(b_p99),
                fmt_ms(a_p99),
                fmt_ms(b_max),
                fmt_ms(a_max),
                f"{b_rss:.1f} → {a_rss:.1f} MB",
            )
        )

    print("# Stress sweep comparison\n")
    print("| scenario | rps_before | rps_after | Δrps | p99_before | p99_after | max_before | max_after | rss |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for r in rows:
        print("| " + " | ".join(r) + " |")

    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1], sys.argv[2]))
