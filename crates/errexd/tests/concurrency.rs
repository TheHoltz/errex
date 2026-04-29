//! Concurrency smoke for the single-connection sqlx pool (P3-9).
//!
//! When the pool was sized to 2 connections, readers and the digest
//! writer could run in parallel. With max=1, every query serializes
//! through one connection. The bench harness only drives 4 WS subscribers
//! plus one ingest stream, so it doesn't exercise the "many concurrent
//! dashboard readers + sustained writer" pattern that would surface
//! pool-contention tails.
//!
//! This test fires 16 reader tasks against `project_summaries` /
//! `list_issues_by_project` while one writer task hammers `upsert_batch_with_events`
//! at ~250 batches/sec. We assert that:
//!   * every read completes successfully (no pool exhaustion, no
//!     deadlock — sqlx's pool acquire timeout is multi-second so a
//!     pathological serialization would surface as test hangs/errors)
//!   * the p99 read latency stays under a generous 100 ms (real
//!     queries are sub-ms; any p99 above 100 ms means the pool
//!     queue is the dominant cost, which would mean P3-9 was
//!     premature).

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use errex_proto::{
    Event, ExceptionContainer, ExceptionInfo, Fingerprint, Frame, Level, Stacktrace,
};
use serde_json::json;
use uuid::Uuid;

#[path = "../src/error.rs"]
mod error;
#[path = "../src/store.rs"]
mod store;

use store::{BatchUpsertInput, Store};

fn unique_tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "errexd-concur-{}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        seq,
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create tempdir");
    p
}

fn sample_event(fp_seed: u64) -> Event {
    Event {
        event_id: Uuid::new_v4(),
        timestamp: Utc::now(),
        platform: Some("javascript".into()),
        level: Some(Level::Error),
        environment: None,
        release: None,
        server_name: None,
        message: None,
        exception: Some(ExceptionContainer {
            values: vec![ExceptionInfo {
                ty: Some("TypeError".into()),
                value: Some(format!("err {fp_seed}")),
                module: None,
                stacktrace: Some(Stacktrace {
                    frames: vec![Frame {
                        function: Some("oops".into()),
                        filename: Some(format!("file{}.js", fp_seed % 10)),
                        module: None,
                        lineno: Some((fp_seed % 1000) as u32 + 1),
                        colno: None,
                        in_app: None,
                    }],
                }),
            }],
        }),
        breadcrumbs: None,
        tags: Some(json!({"env": "test"})),
        contexts: None,
        extra: None,
        user: None,
        request: None,
    }
}

#[tokio::test]
async fn pool_one_holds_under_concurrent_readers_and_writer() {
    let dir = unique_tempdir();
    let store = Store::open(&dir.join("errex.db")).await.unwrap();
    store.migrate().await.unwrap();
    store.create_project("p").await.unwrap();

    // Seed a small population so the read queries return non-empty rows.
    for seed in 0..50u64 {
        let fp = Fingerprint::new(format!("seed-{seed}"));
        let now = Utc::now();
        let event_id = Uuid::new_v4().as_simple().to_string();
        let payload = serde_json::to_string(&sample_event(seed)).unwrap();
        let inputs = vec![BatchUpsertInput {
            project: "p",
            fp: &fp,
            title: "TypeError: seed",
            culprit: Some("oops in file.js"),
            level: Some("error"),
            now,
            event_id: &event_id,
            payload: &payload,
        }];
        store.upsert_batch_with_events(&inputs).await.unwrap();
    }

    let store = Arc::new(store);
    let stop_at = Instant::now() + Duration::from_secs(2);

    // Writer: keep firing batched upserts at ~250/s.
    let writer = {
        let store = store.clone();
        tokio::spawn(async move {
            let mut seed = 100u64;
            while Instant::now() < stop_at {
                let fp = Fingerprint::new(format!("hot-{}", seed % 200));
                let now = Utc::now();
                let event_id = Uuid::new_v4().as_simple().to_string();
                let payload = serde_json::to_string(&sample_event(seed)).unwrap();
                let inputs = vec![BatchUpsertInput {
                    project: "p",
                    fp: &fp,
                    title: "TypeError: hot",
                    culprit: Some("oops in file.js"),
                    level: Some("error"),
                    now,
                    event_id: &event_id,
                    payload: &payload,
                }];
                store.upsert_batch_with_events(&inputs).await.unwrap();
                seed = seed.wrapping_add(1);
                tokio::time::sleep(Duration::from_millis(4)).await;
            }
        })
    };

    // 16 readers: alternating between project_summaries and
    // list_issues_by_project. Record per-call latency.
    let mut readers = Vec::new();
    for r in 0..16 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            let mut latencies: Vec<u128> = Vec::with_capacity(2048);
            let mut i = 0u64;
            while Instant::now() < stop_at {
                let t = Instant::now();
                if (r + i) % 2 == 0 {
                    let _ = store.project_summaries().await.unwrap();
                } else {
                    let _ = store.list_issues_by_project("p").await.unwrap();
                }
                latencies.push(t.elapsed().as_micros());
                i += 1;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            latencies
        }));
    }

    let mut all: Vec<u128> = Vec::new();
    for r in readers {
        all.extend(r.await.unwrap());
    }
    writer.await.unwrap();

    assert!(
        !all.is_empty(),
        "readers produced no samples — test misconfigured"
    );
    all.sort_unstable();
    let p50 = all[all.len() / 2];
    let p99 = all[all.len() * 99 / 100];
    let max = *all.last().unwrap();

    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        p99 < 100_000,
        "pool=1 read p99 too high: p50={p50}us p99={p99}us max={max}us \
         (count={}). If this trips on the bench host but not under CI, \
         the writer is dominating the connection — bump max_connections \
         back to 2 in store::open.",
        all.len(),
    );
}
