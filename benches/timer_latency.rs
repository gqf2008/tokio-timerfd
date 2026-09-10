use std::time::{Duration, Instant};

use futures::stream::StreamExt;
use tokio_timerfd::{Delay, Interval};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[derive(Debug)]
struct Stats {
    min_ns: i128,
    mean_ns: i128,
    p50_ns: i128,
    p95_ns: i128,
    p99_ns: i128,
    max_ns: i128,
}

fn stats(mut samples: Vec<i128>) -> Stats {
    samples.sort_unstable();
    let min_ns = samples[0];
    let max_ns = samples[samples.len() - 1];
    let mean_ns = samples.iter().sum::<i128>() / samples.len() as i128;
    let percentile = |fraction: f64| {
        let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
        samples[index]
    };

    Stats {
        min_ns,
        mean_ns,
        p50_ns: percentile(0.50),
        p95_ns: percentile(0.95),
        p99_ns: percentile(0.99),
        max_ns,
    }
}

async fn measure_delay(period: Duration, iterations: u64) -> Vec<i128> {
    let mut samples = Vec::with_capacity(iterations as usize);

    for _ in 0..iterations {
        let deadline = Instant::now() + period;
        let delay = Delay::new(deadline).expect("failed to create OS timer");
        delay.await.expect("OS timer failed");
        samples.push(
            Instant::now()
                .saturating_duration_since(deadline)
                .as_nanos() as i128,
        );
    }

    samples
}

async fn measure_interval(period: Duration, iterations: u64) -> Vec<i128> {
    let mut interval = Interval::new_interval(period).expect("failed to create OS interval");
    let mut samples = Vec::with_capacity(iterations as usize);
    let mut previous = None;

    for _ in 0..iterations {
        interval.next().await.expect("interval ended").unwrap();
        let now = Instant::now();
        if let Some(previous) = previous.replace(now) {
            let actual = now.saturating_duration_since(previous).as_nanos() as i128;
            samples.push(actual - period.as_nanos() as i128);
        }
    }

    samples
}

async fn measure_tokio_delay(period: Duration, iterations: u64) -> Vec<i128> {
    let mut samples = Vec::with_capacity(iterations as usize);

    for _ in 0..iterations {
        let deadline = Instant::now() + period;
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        samples.push(
            Instant::now()
                .saturating_duration_since(deadline)
                .as_nanos() as i128,
        );
    }

    samples
}

async fn measure_tokio_interval(period: Duration, iterations: u64) -> Vec<i128> {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut samples = Vec::with_capacity(iterations as usize);
    let mut previous = None;

    for _ in 0..iterations {
        interval.tick().await;
        let now = Instant::now();
        if let Some(previous) = previous.replace(now) {
            let actual = now.saturating_duration_since(previous).as_nanos() as i128;
            samples.push(actual - period.as_nanos() as i128);
        }
    }

    samples
}

fn print_stats(name: &str, period_us: u64, iterations: u64, stats: &Stats) {
    println!(
        "{name},{period_us},{iterations},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
        stats.mean_ns as f64 / 1_000.0,
        stats.p50_ns as f64 / 1_000.0,
        stats.p95_ns as f64 / 1_000.0,
        stats.p99_ns as f64 / 1_000.0,
        stats.max_ns as f64 / 1_000.0,
        stats.min_ns as f64 / 1_000.0,
    );
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let iterations = env_u64("BENCH_ITERATIONS", 500);
    let periods_us = [100_u64, 500, 1_000, 5_000, 10_000];

    println!(
        "os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("backend,period_us,iterations,mean_us,p50_us,p95_us,p99_us,max_us,min_us");

    for period_us in periods_us {
        let period = Duration::from_micros(period_us);

        let samples = measure_delay(period, iterations).await;
        print_stats(
            "tokio-timerfd-delay",
            period_us,
            iterations,
            &stats(samples),
        );

        let samples = measure_interval(period, iterations).await;
        print_stats(
            "tokio-timerfd-interval",
            period_us,
            iterations,
            &stats(samples),
        );

        let samples = measure_tokio_delay(period, iterations).await;
        print_stats("tokio-time-delay", period_us, iterations, &stats(samples));

        let samples = measure_tokio_interval(period, iterations).await;
        print_stats(
            "tokio-time-interval",
            period_us,
            iterations,
            &stats(samples),
        );
    }
}
