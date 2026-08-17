use std::{
    fs,
    io::{ErrorKind, Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use tempfile::tempdir;

#[test]
fn manages_sources_only_inside_explicit_temporary_home() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "node",
            "mirror",
            "--base-url",
            "https://mirror.example/node",
        ],
    )
    .assert_success("added node mirror");
    pinset(&home, &["source", "use", "node", "mirror"]).assert_success("active node mirror");
    pinset(&home, &["source", "fallback", "node", "official"])
        .assert_success("fallback node official");

    let listed = pinset(&home, &["source", "list", "node"]);
    listed.assert_success_contains("node mirror custom active https://mirror.example/node/");
    listed.assert_success_contains("node official official fallback:1 https://nodejs.org/dist/");

    let config = fs::read_to_string(home.join("sources.toml")).expect("source config");
    assert!(config.contains("active = \"mirror\""));
    assert!(config.contains("fallback = [\"official\"]"));
    assert!(!home.join("installs").exists());
    assert!(!home.join("shims").exists());
}

#[test]
fn trusted_metadata_mirrors_require_https_and_are_visible_in_source_list() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "node",
            "trusted",
            "--base-url",
            "https://mirror.example/node/",
            "--trust-metadata",
        ],
    )
    .assert_success("added node trusted");
    let listed = pinset(&home, &["source", "list", "node"]);
    listed.assert_success_contains(
        "node trusted custom - https://mirror.example/node/ trusted-metadata",
    );

    let rejected = pinset(
        &home,
        &[
            "source",
            "add",
            "node",
            "insecure",
            "--base-url",
            "http://127.0.0.1:8080/node/",
            "--trust-metadata",
        ],
    );
    assert!(!rejected.success);
    assert!(rejected.stderr.contains("invalid base URL"));
}

#[test]
fn removes_only_inactive_unreferenced_custom_sources() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "python",
            "corp",
            "--base-url",
            "https://packages.example/python/",
        ],
    )
    .assert_success("added python corp");
    pinset(&home, &["source", "remove", "python", "corp"]).assert_success("removed python corp");

    let listed = pinset(&home, &["source", "list", "python"]);
    listed.assert_success_contains(
        "python official official active https://github.com/astral-sh/python-build-standalone/releases/download/",
    );
    assert!(!listed.stdout.contains("corp"));
    assert!(!home.join("installs").exists());
}

#[test]
fn failed_remove_does_not_rewrite_active_source_config() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");

    pinset(
        &home,
        &[
            "source",
            "add",
            "flutter",
            "mirror",
            "--base-url",
            "https://mirror.example/flutter/",
        ],
    )
    .assert_success("added flutter mirror");
    pinset(&home, &["source", "use", "flutter", "mirror"]).assert_success("active flutter mirror");
    let before = fs::read(home.join("sources.toml")).expect("source config before failure");

    let failed = pinset(&home, &["source", "remove", "flutter", "mirror"]);
    assert!(!failed.success, "active source removal must fail");
    assert!(failed.stderr.contains("currently active"));
    let after = fs::read(home.join("sources.toml")).expect("source config after failure");
    assert_eq!(after, before);
    assert!(!home.join("installs").exists());
}

#[test]
fn tests_a_node_source_read_only_against_its_version_index() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");
    let index = br#"[{"version":"v24.19.0","date":"2026-04-01","files":["win-x64-zip","linux-x64","linux-arm64","osx-x64-tar","osx-arm64-tar"],"lts":"Krypton","security":false}]"#.to_vec();
    let signed_manifest =
        include_bytes!("../../pinset-core/tests/fixtures/node-v24.19.0-SHASUMS256.txt.asc")
            .to_vec();
    let (base_url, server) = serve_sequence(vec![
        ("GET /index.json", index),
        ("GET /v24.19.0/SHASUMS256.txt.asc", signed_manifest),
    ]);

    pinset(
        &home,
        &[
            "source",
            "add",
            "node",
            "local",
            "--base-url",
            &base_url,
            "--allow-insecure",
        ],
    )
    .assert_success("added node local");
    let tested = pinset(
        &home,
        &["--lang", "zh-CN", "source", "test", "node", "local"],
    );
    server.join().expect("server");
    tested.assert_success_contains("安装源测试通过");
    tested.assert_success_contains("稳定版本数=1");
    assert!(!home.join("installs").exists());
}

#[test]
fn tests_a_go_source_read_only_against_its_download_index() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");
    let hash = "ab".repeat(32);
    let index = serde_json::json!([{
        "version": "go1.25.1",
        "stable": true,
        "files": [
            {"filename":"go1.25.1.windows-amd64.zip","os":"windows","arch":"amd64","version":"go1.25.1","sha256":hash.clone(),"size":1,"kind":"archive"},
            {"filename":"go1.25.1.darwin-arm64.tar.gz","os":"darwin","arch":"arm64","version":"go1.25.1","sha256":hash.clone(),"size":1,"kind":"archive"},
            {"filename":"go1.25.1.darwin-amd64.tar.gz","os":"darwin","arch":"amd64","version":"go1.25.1","sha256":hash.clone(),"size":1,"kind":"archive"},
            {"filename":"go1.25.1.linux-arm64.tar.gz","os":"linux","arch":"arm64","version":"go1.25.1","sha256":hash.clone(),"size":1,"kind":"archive"},
            {"filename":"go1.25.1.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.25.1","sha256":hash,"size":1,"kind":"archive"}
        ]
    }])
    .to_string()
    .into_bytes();
    let (base_url, server) = serve_sequence(vec![("GET /?mode=json&include=all", index)]);

    pinset(
        &home,
        &[
            "source",
            "add",
            "go",
            "local",
            "--base-url",
            &base_url,
            "--allow-insecure",
        ],
    )
    .assert_success("added go local");
    let tested = pinset(&home, &["--lang", "zh-CN", "source", "test", "go", "local"]);
    server.join().expect("server");
    tested.assert_success_contains("安装源测试通过");
    tested.assert_success_contains("稳定版本数=1");
    assert!(!home.join("installs").exists());
}

#[test]
fn tests_a_flutter_source_read_only_against_all_platform_indexes() {
    let root = tempdir().expect("temporary PINSET_HOME");
    let home = root.path().join("isolated-home");
    let version = "3.47.0";
    let release_hash = "cd".repeat(20);
    let sha256 = "ab".repeat(32);
    let entry = |archive: &str, dart_arch: &str| {
        serde_json::json!({
            "hash": release_hash.clone(),
            "channel": "stable",
            "version": version,
            "dart_sdk_version": "3.13.0",
            "dart_sdk_arch": dart_arch,
            "archive": archive,
            "sha256": sha256.clone()
        })
    };
    let linux = serde_json::json!({"releases": [entry(
        "stable/linux/flutter_linux_3.47.0-stable.tar.xz",
        "x64"
    )]})
    .to_string()
    .into_bytes();
    let windows = serde_json::json!({"releases": [entry(
        "stable/windows/flutter_windows_3.47.0-stable.zip",
        "x64"
    )]})
    .to_string()
    .into_bytes();
    let macos = serde_json::json!({"releases": [
        entry("stable/macos/flutter_macos_3.47.0-stable.zip", "x64"),
        entry("stable/macos/flutter_macos_arm64_3.47.0-stable.zip", "arm64")
    ]})
    .to_string()
    .into_bytes();
    let (base_url, server) = serve_sequence(vec![
        (
            "GET /flutter_infra_release/releases/releases_linux.json",
            linux,
        ),
        (
            "GET /flutter_infra_release/releases/releases_windows.json",
            windows,
        ),
        (
            "GET /flutter_infra_release/releases/releases_macos.json",
            macos,
        ),
    ]);

    pinset(
        &home,
        &[
            "source",
            "add",
            "flutter",
            "local",
            "--base-url",
            &base_url,
            "--allow-insecure",
        ],
    )
    .assert_success("added flutter local");
    let tested = pinset(
        &home,
        &["--lang", "zh-CN", "source", "test", "flutter", "local"],
    );
    server.join().expect("server");
    tested.assert_success_contains("安装源测试通过");
    tested.assert_success_contains("稳定版本数=1");
    assert!(!home.join("installs").exists());
}

fn serve_sequence(responses: Vec<(&'static str, Vec<u8>)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    listener
        .set_nonblocking(true)
        .expect("configure nonblocking server");
    let address = listener.local_addr().expect("server address");
    let server = thread::spawn(move || {
        for (expected, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        panic!("timed out waiting for request {expected}");
                    }
                    Err(error) => panic!("accept request: {error}"),
                }
            };
            let mut request = [0_u8; 2048];
            let count = stream.read(&mut request).expect("read request");
            assert!(String::from_utf8_lossy(&request[..count]).contains(expected));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("response headers");
            stream.write_all(&body).expect("response body");
        }
    });
    (format!("http://{address}/"), server)
}

fn pinset(home: &std::path::Path, arguments: &[&str]) -> CommandResult {
    let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .env("PINSET_HOME", home)
        .output()
        .expect("run pinset");
    CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    }
}

struct CommandResult {
    success: bool,
    stdout: String,
    stderr: String,
}

impl CommandResult {
    fn assert_success(&self, expected: &str) {
        assert!(
            self.success && self.stdout.contains(expected),
            "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
            self.stdout,
            self.stderr
        );
    }

    fn assert_success_contains(&self, expected: &str) {
        self.assert_success(expected);
    }
}
