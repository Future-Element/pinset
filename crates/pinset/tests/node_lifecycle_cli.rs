use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn uninstall_refuses_active_project_reference_and_force_removes_owned_install() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"24.1.0\"\n",
    )
    .expect("project config");
    create_install_receipt(&home, "24.1.0", "linux-x86_64");

    let refused = pinset(&project, &home, &["uninstall", "node@24.1.0"]);
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("refusing to uninstall Node.js 24.1.0"));

    let removed = pinset(
        &project,
        &home,
        &["--lang", "zh-CN", "uninstall", "node@24.1.0", "--force"],
    );
    assert_success_contains(&removed, "已卸载 Node.js 24.1.0");
    assert!(!home.join("installs/node/24.1.0").exists());
    assert!(project.join("pinset.toml").is_file());
}

#[test]
fn uninstall_rejects_floating_versions_and_unowned_directories() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir(&project).expect("project");

    let floating = pinset(&project, &home, &["uninstall", "node@24"]);
    assert!(!floating.status.success());
    assert!(stderr(&floating).contains("invalid exact Node.js version"));

    let directory = home.join("installs/node/24.1.0/linux-x86_64");
    fs::create_dir_all(&directory).expect("unowned directory");
    fs::write(directory.join("keep.txt"), b"foreign").expect("foreign file");
    let unowned = pinset(&project, &home, &["uninstall", "node@24.1.0", "--force"]);
    assert!(!unowned.status.success());
    assert!(stderr(&unowned).contains("unsafe or unowned"));
    assert!(directory.join("keep.txt").is_file());
}

#[test]
fn cache_list_and_clean_leave_unknown_files_untouched() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    let cache = home.join("downloads/sha256");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(&cache).expect("cache");
    let hash = "a".repeat(64);
    fs::write(cache.join(format!("{hash}.archive")), b"archive").expect("archive");
    fs::write(cache.join("keep.txt"), b"unknown").expect("unknown");

    let listed = pinset(&project, &home, &["cache", "list"]);
    assert_success_contains(&listed, &format!("{hash} cached bytes=7"));

    let cleaned = pinset(&project, &home, &["--lang=zh-CN", "cache", "clean"]);
    assert_success_contains(&cleaned, "已清理 1 个缓存归档（7 字节）");
    assert!(cache.join("keep.txt").is_file());
    assert!(!cache.join(format!("{hash}.archive")).exists());
}

#[test]
fn cache_import_requires_and_records_the_declared_sha256() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    let archive = root.path().join("node.tar.xz");
    fs::create_dir_all(&project).expect("project");
    fs::write(&archive, b"offline archive").expect("archive");
    let hash = "057057782a64b95b5932387e720906f95b9524d21984e0494f0db565abf37c8b";

    let imported = pinset(
        &project,
        &home,
        &[
            "cache",
            "import",
            archive.to_str().expect("UTF-8 archive"),
            "--sha256",
            hash,
        ],
    );
    assert_success_contains(&imported, hash);
    assert_eq!(
        fs::read(home.join(format!("downloads/sha256/{hash}.archive"))).expect("cached archive"),
        b"offline archive"
    );

    let rejected = pinset(
        &project,
        &home,
        &[
            "cache",
            "import",
            archive.to_str().expect("UTF-8 archive"),
            "--sha256",
            &"0".repeat(64),
        ],
    );
    assert!(!rejected.status.success());
    assert!(stderr(&rejected).contains("SHA-256 mismatch"));
}

fn create_install_receipt(home: &Path, version: &str, target: &str) {
    let directory = home
        .join("installs")
        .join("node")
        .join(version)
        .join(target);
    fs::create_dir_all(&directory).expect("install directory");
    fs::write(
        directory.join(".pinset-install.toml"),
        format!(
            "schema = 1\ncomplete = true\ntool = \"node\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
        ),
    )
    .expect("install receipt");
}

fn pinset(cwd: &Path, home: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .current_dir(cwd)
        .env("PINSET_HOME", home)
        .env_remove("PINSET_LANG")
        .output()
        .expect("run pinset")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success_contains(output: &std::process::Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
