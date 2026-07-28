use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use pinset_core::{current_target, install_shims};
use tempfile::tempdir;

fn main() {
    let iterations = env::args()
        .nth(1)
        .map(|value| value.parse::<usize>().expect("iterations must be a number"))
        .unwrap_or(300);
    let (pinset_binary, shim_binary) = release_binaries();
    assert!(
        pinset_binary.is_file(),
        "build the release pinset binary first: {}",
        pinset_binary.display()
    );
    assert!(
        shim_binary.is_file(),
        "build the release shim binary first: {}",
        shim_binary.display()
    );

    let root = tempdir().expect("temporary benchmark directory");
    let project = root.path().join("project").join("packages").join("app");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        root.path().join("project").join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");

    let runtime = runtime_path(&home);
    fs::create_dir_all(runtime.parent().expect("runtime parent")).expect("runtime directory");
    fs::copy(&pinset_binary, &runtime).expect("fake runtime binary");

    let installed = install_shims(&shim_binary, &home.join("shims"), &["node".to_owned()])
        .expect("install benchmark shim");
    let node_shim = &installed[0].destination;

    for _ in 0..20 {
        run(&runtime, &project, &home);
        run(node_shim, &project, &home);
    }

    let mut direct = Vec::with_capacity(iterations);
    let mut shimmed = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        direct.push(sample(|| run(&runtime, &project, &home)));
        shimmed.push(sample(|| run(node_shim, &project, &home)));
    }
    direct.sort_unstable();
    shimmed.sort_unstable();

    let direct_p95 = percentile(&direct, 0.95);
    let shim_p95 = percentile(&shimmed, 0.95);
    println!("iterations={iterations}");
    println!("direct_median_us={}", micros(percentile(&direct, 0.50)));
    println!("direct_p95_us={}", micros(direct_p95));
    println!("shimmed_median_us={}", micros(percentile(&shimmed, 0.50)));
    println!("shimmed_p95_us={}", micros(shim_p95));
    println!(
        "estimated_p95_overhead_us={}",
        micros(shim_p95.saturating_sub(direct_p95))
    );
}

fn release_binaries() -> (PathBuf, PathBuf) {
    let executable = env::current_exe().expect("current example path");
    let release_dir = executable
        .parent()
        .and_then(Path::parent)
        .expect("target release directory");
    let suffix = env::consts::EXE_SUFFIX;
    (
        release_dir.join(format!("pinset{suffix}")),
        release_dir.join(format!("pinset-shim{suffix}")),
    )
}

fn runtime_path(home: &Path) -> PathBuf {
    home.join("installs")
        .join("node")
        .join("20.0.0")
        .join(current_target())
        .join("bin")
        .join(format!("node{}", env::consts::EXE_SUFFIX))
}

fn run(executable: &Path, cwd: &Path, home: &Path) {
    let output = Command::new(executable)
        .arg("--help")
        .current_dir(cwd)
        .env("PINSET_HOME", home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("benchmark process");
    assert!(
        output.success(),
        "{} failed with {output}",
        executable.display(),
    );
}

fn sample(operation: impl FnOnce()) -> Duration {
    let started = Instant::now();
    operation();
    started.elapsed()
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index]
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}
