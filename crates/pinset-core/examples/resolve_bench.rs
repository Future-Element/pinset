use std::{
    env, fs,
    time::{Duration, Instant},
};

use pinset_core::{current_target, resolve_command};
use tempfile::tempdir;

fn main() {
    let iterations = env::args()
        .nth(1)
        .map(|value| value.parse::<usize>().expect("iterations must be a number"))
        .unwrap_or(2_000);
    let root = tempdir().expect("temporary benchmark directory");
    let project = root.path().join("project");
    let nested = project.join("packages").join("app").join("src");
    let home = root.path().join("home");
    fs::create_dir_all(&nested).expect("nested project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    let bin = home
        .join("installs")
        .join("node")
        .join("20.0.0")
        .join(current_target())
        .join("bin");
    fs::create_dir_all(&bin).expect("runtime bin");
    let executable = if cfg!(windows) {
        bin.join("node.exe")
    } else {
        bin.join("node")
    };
    fs::write(executable, b"benchmark fixture").expect("runtime fixture");

    for _ in 0..100 {
        resolve_command("node", &nested, &home).expect("warmup resolution");
    }

    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        resolve_command("node", &nested, &home).expect("resolution");
        samples.push(started.elapsed());
    }
    samples.sort_unstable();

    println!("iterations={iterations}");
    println!("median_us={}", micros(percentile(&samples, 0.50)));
    println!("p95_us={}", micros(percentile(&samples, 0.95)));
    println!("p99_us={}", micros(percentile(&samples, 0.99)));
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index]
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}
