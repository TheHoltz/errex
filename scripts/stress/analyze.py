#!/usr/bin/env python3
"""Aggregate stress-test scenario reports into a single Markdown summary.

Reads every *.json in scripts/stress/results/, prints a table sorted by
scenario, and flags scenarios where the daemon failed to keep up (achieved
RPS << target, ingest p99 > 100 ms, server errors > 0, or WS broadcast
visibly lagged).
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

RESULTS = Path(__file__).parent / "results"

# Per-scenario thresholds for the "ok" column. These are heuristics that
# track our self-host budget — small machine, sub-100ms p99 desired, no
# server errors at any tested rate.
P99_INGEST_BUDGET_MS = 100.0
P99_WS_BUDGET_MS = 250.0
ACHIEVED_RPS_FRACTION = 0.85


def load() -> list[dict]:
    out = []
    for p in sorted(RESULTS.glob("*.json")):
        try:
            out.append(json.load(open(p)))
        except Exception as e:
            print(f"[skip] {p.name}: {e}", file=sys.stderr)
    return out


def fmt_ms(v) -> str:
    if v is None:
        return "—"
    if v < 1.0:
        return f"{v*1000:.0f}µs"
    if v < 10.0:
        return f"{v:.2f}ms"
    return f"{v:.1f}ms"


def main():
    runs = load()
    if not runs:
        print("no results found in", RESULTS)
        return 1

    print("# errexd stress-test results\n")
    print(f"_{len(runs)} scenarios_\n")

    # Header.
    cols = [
        "scenario",
        "target_rps",
        "achieved_rps",
        "sent",
        "2xx",
        "429",
        "5xx",
        "io_err",
        "ingest_p50",
        "ingest_p99",
        "ingest_max",
        "ws_recv",
        "ws_p50",
        "ws_p99",
        "ws_max",
        "rss_max_mb",
        "db_kb",
        "verdict",
    ]
    print("| " + " | ".join(cols) + " |")
    print("|" + "---|" * len(cols))

    findings: list[str] = []
    for r in runs:
        cfg = r.get("config", {})
        target = cfg.get("rps", 0)
        achieved = r.get("achieved_rps", 0)
        ingest = r.get("ingest_latency_ms", {})
        ws = r.get("ws_lag_ms", {})
        rss = r.get("daemon_rss_kb", {})
        rss_max = rss.get("max", 0)
        rss_mb = rss_max / 1024 if rss_max else 0
        db = r.get("db_size_bytes")
        db_kb = (db / 1024) if db else 0

        problems = []
        if target > 0 and achieved < target * ACHIEVED_RPS_FRACTION:
            problems.append(
                f"achieved_rps {achieved:.0f} < {ACHIEVED_RPS_FRACTION*100:.0f}% of target {target}"
            )
        if r.get("err_5xx", 0) > 0:
            problems.append(f"{r['err_5xx']} 5xx responses")
        if r.get("err_io", 0) > 0:
            problems.append(f"{r['err_io']} io errors")
        if ingest.get("p99", 0) > P99_INGEST_BUDGET_MS:
            problems.append(
                f"ingest p99 {ingest['p99']:.0f}ms > {P99_INGEST_BUDGET_MS:.0f}ms"
            )
        if ws.get("count", 0) > 0 and ws.get("p99", 0) > P99_WS_BUDGET_MS:
            problems.append(f"ws p99 {ws['p99']:.0f}ms > {P99_WS_BUDGET_MS:.0f}ms")

        verdict = "OK" if not problems else "REVIEW"

        row = [
            r.get("label", "?"),
            str(target),
            f"{achieved:.0f}",
            str(r.get("sent", 0)),
            str(r.get("ok_2xx", 0)),
            str(r.get("err_429", 0)),
            str(r.get("err_5xx", 0)),
            str(r.get("err_io", 0)),
            fmt_ms(ingest.get("p50")),
            fmt_ms(ingest.get("p99")),
            fmt_ms(ingest.get("max")),
            str(ws.get("count", 0)),
            fmt_ms(ws.get("p50")) if ws.get("count") else "—",
            fmt_ms(ws.get("p99")) if ws.get("count") else "—",
            fmt_ms(ws.get("max")) if ws.get("count") else "—",
            f"{rss_mb:.1f}",
            f"{db_kb:.0f}",
            verdict,
        ]
        print("| " + " | ".join(row) + " |")

        if problems:
            findings.append(
                f"- **{r.get('label','?')}**: " + "; ".join(problems)
            )

    print("\n## Findings\n")
    if findings:
        print("\n".join(findings))
    else:
        print("All scenarios met the budget thresholds.")

    return 0


if __name__ == "__main__":
    sys.exit(main())
