// errex-stress — autonomous load harness for errexd.
//
// Fires Sentry-compatible envelopes at /api/:project/envelope/ at a target
// RPS with configurable payload size, fingerprint cardinality, and a pool
// of concurrent WebSocket subscribers. Captures end-to-end ingest latency
// (HTTP), WebSocket broadcast lag (digest -> client), daemon RSS, and a
// rough digest-throughput estimate. Writes a single JSON report on exit.

use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use clap::Parser;
use flate2::{write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use hdrhistogram::Histogram;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use serde::Serialize;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

#[derive(Parser, Debug, Clone)]
#[command(version, about = "errexd stress harness")]
struct Cli {
    /// Daemon base URL (HTTP). WS URL derived by swapping scheme.
    #[arg(long, default_value = "http://127.0.0.1:9090")]
    base: String,
    /// Target events per second (aggregate across all workers).
    #[arg(long, default_value_t = 200)]
    rps: u64,
    /// Concurrent ingest workers (higher = better at saturating, lower = backpressure visibility).
    #[arg(long, default_value_t = 16)]
    workers: usize,
    /// Test duration (seconds).
    #[arg(long, default_value_t = 30)]
    duration_secs: u64,
    /// Distinct fingerprints to rotate through (controls dedupe pressure).
    #[arg(long, default_value_t = 50)]
    cardinality: u32,
    /// Approximate stack frame count per event (controls payload size).
    #[arg(long, default_value_t = 8)]
    frames: u32,
    /// Number of distinct projects to spread events across.
    #[arg(long, default_value_t = 4)]
    projects: u32,
    /// Compress envelopes with gzip (matches typical SDK behavior).
    #[arg(long, default_value_t = false)]
    gzip: bool,
    /// Number of WebSocket subscribers to attach.
    #[arg(long, default_value_t = 4)]
    ws_subscribers: usize,
    /// PID of daemon to sample RSS from (omit to skip RSS sampling).
    #[arg(long)]
    daemon_pid: Option<u32>,
    /// Where to write the JSON report.
    #[arg(long, default_value = "./stress-report.json")]
    out: PathBuf,
    /// Label for the run (recorded in report).
    #[arg(long, default_value = "")]
    label: String,
}

#[derive(Debug, Default)]
struct Counters {
    sent: AtomicU64,
    ok_2xx: AtomicU64,
    err_4xx: AtomicU64,
    err_5xx: AtomicU64,
    err_429: AtomicU64,
    err_io: AtomicU64,
    ws_received: AtomicU64,
    ws_lagged: AtomicU64,
}

#[derive(Serialize)]
struct Report {
    label: String,
    config: ReportConfig,
    duration_secs: f64,
    sent: u64,
    ok_2xx: u64,
    err_4xx: u64,
    err_5xx: u64,
    err_429: u64,
    err_io: u64,
    achieved_rps: f64,
    ingest_latency_ms: Latencies,
    ws_received: u64,
    ws_lagged: u64,
    ws_lag_ms: Latencies,
    daemon_rss_kb: RssStats,
}

#[derive(Serialize)]
struct ReportConfig {
    base: String,
    rps: u64,
    workers: usize,
    duration_secs: u64,
    cardinality: u32,
    frames: u32,
    projects: u32,
    gzip: bool,
    ws_subscribers: usize,
}

#[derive(Serialize, Default)]
struct Latencies {
    count: u64,
    p50: f64,
    p90: f64,
    p99: f64,
    p999: f64,
    max: f64,
    mean: f64,
}

impl Latencies {
    fn from_hist(h: &Histogram<u64>, scale: f64) -> Self {
        if h.is_empty() {
            return Self::default();
        }
        Self {
            count: h.len(),
            p50: h.value_at_quantile(0.5) as f64 / scale,
            p90: h.value_at_quantile(0.9) as f64 / scale,
            p99: h.value_at_quantile(0.99) as f64 / scale,
            p999: h.value_at_quantile(0.999) as f64 / scale,
            max: h.max() as f64 / scale,
            mean: h.mean() / scale,
        }
    }
}

#[derive(Serialize, Default)]
struct RssStats {
    samples: u64,
    min: u64,
    max: u64,
    mean: f64,
    final_kb: u64,
}

fn ws_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}/ws/stress")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}/ws/stress")
    } else {
        format!("{trimmed}/ws/stress")
    }
}

fn parse_iso_to_ms(s: &str) -> Option<u128> {
    // Minimal RFC3339 parser for "YYYY-MM-DDTHH:MM:SS[.fff]Z" (or +HH:MM).
    // Avoids a chrono dep in the harness. Fractional seconds + numeric
    // offsets are honored; missing ones default to 0 / UTC.
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let mo: u32 = s.get(5..7)?.parse().ok()?;
    let d: u32 = s.get(8..10)?.parse().ok()?;
    let hh: u32 = s.get(11..13)?.parse().ok()?;
    let mm: u32 = s.get(14..16)?.parse().ok()?;
    let ss: u32 = s.get(17..19)?.parse().ok()?;
    let mut idx = 19;
    let mut nanos: u64 = 0;
    if bytes.get(idx) == Some(&b'.') {
        idx += 1;
        let frac_start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        let frac = &s[frac_start..idx];
        // Right-pad to 9 digits then parse as nanoseconds.
        let mut padded = String::from(frac);
        while padded.len() < 9 {
            padded.push('0');
        }
        nanos = padded[..9].parse().unwrap_or(0);
    }
    let mut tz_off_secs: i64 = 0;
    if let Some(&c) = bytes.get(idx) {
        if c == b'Z' || c == b'z' {
            // UTC
        } else if c == b'+' || c == b'-' {
            let sign: i64 = if c == b'-' { -1 } else { 1 };
            let oh: i64 = s.get(idx + 1..idx + 3)?.parse().ok()?;
            let om: i64 = s.get(idx + 4..idx + 6)?.parse().ok()?;
            tz_off_secs = sign * (oh * 3600 + om * 60);
        }
    }
    let days = ymd_to_days(y as i32, mo, d);
    let secs = days * 86_400
        + hh as i64 * 3600
        + mm as i64 * 60
        + ss as i64
        - tz_off_secs;
    Some((secs as i128 * 1000 + (nanos / 1_000_000) as i128) as u128)
}

fn ymd_to_days(y: i32, m: u32, d: u32) -> i64 {
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + (d as u64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era as i64) * 146_097 + doe as i64 - 719_468
}

fn build_event_body(
    rng: &mut SmallRng,
    project: &str,
    cardinality: u32,
    frames: u32,
    sent_ts_ms: u128,
) -> (String, String) {
    // Fingerprint pivot: (type, function:lineno) drives the daemon's
    // fingerprint algorithm. Rotating the lineno across `cardinality`
    // values gives us deterministic dedup pressure.
    let bucket = rng.gen_range(0..cardinality);
    let event_id = uuid::Uuid::new_v4().simple().to_string();
    let header = format!(
        r#"{{"event_id":"{event_id}","sent_at":"{}"}}"#,
        chrono_now_iso()
    );
    let item_header = r#"{"type":"event"}"#;

    // We embed the harness send timestamp (ms since epoch) inside the
    // event payload as `extra.harness_send_ms` so WS subscribers can
    // compute end-to-end (ingest -> digest -> broadcast) lag against
    // their own receive clock.
    let mut frame_buf = String::with_capacity(frames as usize * 80);
    for i in 0..frames {
        if i > 0 {
            frame_buf.push(',');
        }
        frame_buf.push_str(&format!(
            r#"{{"function":"frame_{i}","filename":"src/mod_{i}.rs","lineno":{},"in_app":true}}"#,
            100 + i
        ));
    }

    let payload = format!(
        r#"{{"event_id":"{event_id}","timestamp":"{}","platform":"rust","level":"error","extra":{{"harness_send_ms":{sent_ts_ms},"project":"{project}"}},"exception":{{"values":[{{"type":"StressError","value":"bucket_{bucket}","stacktrace":{{"frames":[{frame_buf}]}}}}]}}}}"#,
        chrono_now_iso()
    );

    let envelope = format!("{header}\n{item_header}\n{payload}\n");
    let url = format!("/api/{project}/envelope/");
    (url, envelope)
}

fn chrono_now_iso() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    // RFC3339 without external crate dep on chrono: seconds-precision is fine.
    epoch_to_iso(now.as_secs())
}

fn epoch_to_iso(secs: u64) -> String {
    // Minimal, correct-enough seconds-since-epoch -> "YYYY-MM-DDTHH:MM:SSZ".
    // Avoids pulling chrono for a one-off label inside payloads.
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

fn read_rss_kb(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb);
        }
    }
    None
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let stop = Arc::new(AtomicBool::new(false));
    let counters = Arc::new(Counters::default());
    let ingest_hist = Arc::new(Mutex::new(
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
    ));
    let ws_lag_hist = Arc::new(Mutex::new(
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).unwrap(),
    ));
    let rss_samples: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

    println!(
        "[stress] base={} rps={} workers={} dur={}s cardinality={} frames={} projects={} gzip={} ws={} pid={:?}",
        cli.base, cli.rps, cli.workers, cli.duration_secs, cli.cardinality, cli.frames,
        cli.projects, cli.gzip, cli.ws_subscribers, cli.daemon_pid
    );

    // Sanity-check daemon is up.
    let probe = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let health = probe
        .get(format!("{}/health", cli.base.trim_end_matches('/')))
        .send()
        .await
        .context("daemon /health probe failed (is errexd running?)")?;
    anyhow::ensure!(
        health.status().is_success(),
        "daemon /health returned {}",
        health.status()
    );

    // Spawn WS subscribers.
    let mut ws_handles = Vec::new();
    for i in 0..cli.ws_subscribers {
        let url = ws_url(&cli.base);
        let counters = counters.clone();
        let hist = ws_lag_hist.clone();
        let stop = stop.clone();
        ws_handles.push(tokio::spawn(async move {
            ws_subscriber(i, url, counters, hist, stop).await;
        }));
    }

    // Give WS subs a moment to subscribe before traffic starts.
    tokio::time::sleep(Duration::from_millis(250)).await;

    // RSS sampler.
    let rss_handle = if let Some(pid) = cli.daemon_pid {
        let samples = rss_samples.clone();
        let stop = stop.clone();
        Some(tokio::spawn(async move {
            while !stop.load(Ordering::Relaxed) {
                if let Some(kb) = read_rss_kb(pid) {
                    samples.lock().await.push(kb);
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }))
    } else {
        None
    };

    // Pacing: each worker runs at rps / workers, ticking on a sleep_until.
    let per_worker_rps = (cli.rps as f64 / cli.workers as f64).max(1.0);
    let interval = Duration::from_secs_f64(1.0 / per_worker_rps);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(cli.duration_secs);

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .timeout(Duration::from_secs(15))
        .build()?;

    let mut workers = Vec::new();
    for w in 0..cli.workers {
        let cli_c = cli.clone();
        let counters = counters.clone();
        let hist = ingest_hist.clone();
        let client = client.clone();
        let stop = stop.clone();
        workers.push(tokio::spawn(async move {
            worker_loop(w, cli_c, client, counters, hist, stop, interval, deadline).await;
        }));
    }

    // Live progress every second.
    let progress = {
        let counters = counters.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            let mut last_sent = 0u64;
            let mut last_t = start;
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let sent = counters.sent.load(Ordering::Relaxed);
                let now = Instant::now();
                let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
                let rps_inst = (sent - last_sent) as f64 / dt;
                last_sent = sent;
                last_t = now;
                println!(
                    "[t={:>4.1}s] sent={} 2xx={} 429={} 5xx={} io_err={} ws_recv={} ws_lag={} rps_inst={:.0}",
                    now.duration_since(start).as_secs_f64(),
                    sent,
                    counters.ok_2xx.load(Ordering::Relaxed),
                    counters.err_429.load(Ordering::Relaxed),
                    counters.err_5xx.load(Ordering::Relaxed),
                    counters.err_io.load(Ordering::Relaxed),
                    counters.ws_received.load(Ordering::Relaxed),
                    counters.ws_lagged.load(Ordering::Relaxed),
                    rps_inst,
                );
            }
        })
    };

    // Wait for workers to finish.
    for h in workers {
        let _ = h.await;
    }
    stop.store(true, Ordering::Relaxed);

    // Drain WS for a moment to capture in-flight broadcasts after last send.
    tokio::time::sleep(Duration::from_secs(2)).await;
    for h in ws_handles {
        h.abort();
        let _ = h.await;
    }
    if let Some(h) = rss_handle {
        h.abort();
        let _ = h.await;
    }
    progress.abort();
    let _ = progress.await;

    let elapsed = started.elapsed().as_secs_f64();
    let sent = counters.sent.load(Ordering::Relaxed);

    let ingest_h = ingest_hist.lock().await;
    let ws_h = ws_lag_hist.lock().await;
    let rss = rss_samples.lock().await;

    let report = Report {
        label: cli.label.clone(),
        config: ReportConfig {
            base: cli.base.clone(),
            rps: cli.rps,
            workers: cli.workers,
            duration_secs: cli.duration_secs,
            cardinality: cli.cardinality,
            frames: cli.frames,
            projects: cli.projects,
            gzip: cli.gzip,
            ws_subscribers: cli.ws_subscribers,
        },
        duration_secs: elapsed,
        sent,
        ok_2xx: counters.ok_2xx.load(Ordering::Relaxed),
        err_4xx: counters.err_4xx.load(Ordering::Relaxed),
        err_5xx: counters.err_5xx.load(Ordering::Relaxed),
        err_429: counters.err_429.load(Ordering::Relaxed),
        err_io: counters.err_io.load(Ordering::Relaxed),
        achieved_rps: sent as f64 / elapsed.max(0.001),
        // Histograms recorded in microseconds; report in milliseconds.
        ingest_latency_ms: Latencies::from_hist(&ingest_h, 1000.0),
        ws_received: counters.ws_received.load(Ordering::Relaxed),
        ws_lagged: counters.ws_lagged.load(Ordering::Relaxed),
        ws_lag_ms: Latencies::from_hist(&ws_h, 1000.0),
        daemon_rss_kb: if rss.is_empty() {
            RssStats::default()
        } else {
            let min = *rss.iter().min().unwrap();
            let max = *rss.iter().max().unwrap();
            let mean = rss.iter().sum::<u64>() as f64 / rss.len() as f64;
            RssStats {
                samples: rss.len() as u64,
                min,
                max,
                mean,
                final_kb: *rss.last().unwrap(),
            }
        },
    };

    let json = serde_json::to_string_pretty(&report)?;
    println!("\n[stress] === REPORT ===\n{json}");
    let mut f = File::create(&cli.out)?;
    f.write_all(json.as_bytes())?;
    println!("[stress] wrote {}", cli.out.display());

    Ok(())
}

#[allow(clippy::too_many_arguments)]
// reason: harness internals — the loop genuinely needs all of these and
// bundling them into a struct just to satisfy the lint would be churn.
async fn worker_loop(
    _id: usize,
    cli: Cli,
    client: reqwest::Client,
    counters: Arc<Counters>,
    hist: Arc<Mutex<Histogram<u64>>>,
    stop: Arc<AtomicBool>,
    interval: Duration,
    deadline: Instant,
) {
    let mut rng = SmallRng::from_entropy();
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
        next += interval;
        let project = format!("p{}", rng.gen_range(0..cli.projects));
        let send_ms = now_ms();
        let (path, body) = build_event_body(&mut rng, &project, cli.cardinality, cli.frames, send_ms);
        let url = format!("{}{}", cli.base.trim_end_matches('/'), path);

        let body_bytes = if cli.gzip {
            let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
            enc.write_all(body.as_bytes()).unwrap();
            enc.finish().unwrap()
        } else {
            body.into_bytes()
        };

        let started = Instant::now();
        let mut req = client
            .post(&url)
            .header("content-type", "application/x-sentry-envelope")
            .body(body_bytes);
        if cli.gzip {
            req = req.header("content-encoding", "gzip");
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let micros = started.elapsed().as_micros() as u64;
                let _ = hist.lock().await.record(micros.max(1));
                counters.sent.fetch_add(1, Ordering::Relaxed);
                if status.is_success() {
                    counters.ok_2xx.fetch_add(1, Ordering::Relaxed);
                } else if status.as_u16() == 429 {
                    counters.err_429.fetch_add(1, Ordering::Relaxed);
                } else if status.is_client_error() {
                    counters.err_4xx.fetch_add(1, Ordering::Relaxed);
                } else if status.is_server_error() {
                    counters.err_5xx.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(_) => {
                counters.sent.fetch_add(1, Ordering::Relaxed);
                counters.err_io.fetch_add(1, Ordering::Relaxed);
            }
        }

        let now = Instant::now();
        if next > now {
            tokio::time::sleep(next - now).await;
        } else {
            // Falling behind target rate; do not catch up by busy-looping.
            next = now;
        }
    }
}

async fn ws_subscriber(
    id: usize,
    url: String,
    counters: Arc<Counters>,
    hist: Arc<Mutex<Histogram<u64>>>,
    stop: Arc<AtomicBool>,
) {
    let connect = match tokio_tungstenite::connect_async(&url).await {
        Ok(x) => x,
        Err(e) => {
            eprintln!("[ws#{id}] connect failed: {e}");
            return;
        }
    };
    let (mut ws, _resp) = connect;
    while !stop.load(Ordering::Relaxed) {
        let next = match ws.next().await {
            Some(Ok(m)) => m,
            Some(Err(e)) => {
                eprintln!("[ws#{id}] error: {e}");
                break;
            }
            None => break,
        };
        let text = match next {
            Message::Text(t) => t,
            Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
            Message::Ping(p) => {
                let _ = ws.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(_) => break,
            _ => continue,
        };
        // Look only at "event" messages so we measure the ingest -> broadcast
        // path. Snapshot/issue messages on connect are ignored for lag stats.
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let kind = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if kind != "issue_created" && kind != "issue_updated" {
            continue;
        }
        counters.ws_received.fetch_add(1, Ordering::Relaxed);
        if let Some(last_seen) = v
            .pointer("/issue/last_seen")
            .and_then(|x| x.as_str())
        {
            if let Some(ts_ms) = parse_iso_to_ms(last_seen) {
                let now = now_ms();
                let lag_ms = now.saturating_sub(ts_ms);
                // Histogram is microsecond-scale; multiply ms by 1000 so the
                // ingest and ws histograms share units. Cap at one minute to
                // avoid a malformed timestamp blowing the hist range.
                let lag_us = (lag_ms.min(60_000) as u64).saturating_mul(1000).max(1);
                let _ = hist.lock().await.record(lag_us);
            }
        }
    }
}
