//! Opt-in real SDK acceptance; run inside Docker or a native acceptance runner.
use pinset_core::{EnvironmentProfile, ProjectEnvironment};
use pinset_env::{EnvironmentDocument, generate_identity, write_encrypted_profile};
use secrecy::ExposeSecret;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn run(root: &Path, home: &Path, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(root)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{arguments:?}: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "downloads and executes pinned real Node SDKs; Docker/native acceptance only"]
fn real_git_worktrees_run_different_node_versions_and_profiles_concurrently() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let one = temp.path().join("one");
    let two = temp.path().join("two");
    fs::create_dir(&one).unwrap();
    if let Some(cache) = std::env::var_os("PINSET_ACCEPTANCE_CACHE") {
        let source = Path::new(&cache).join("downloads/sha256");
        let destination = home.join("downloads/sha256");
        fs::create_dir_all(&destination).unwrap();
        if source.is_dir() {
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if entry.file_type().unwrap().is_file()
                    && name.len() == 72
                    && name.ends_with(".archive")
                    && name[..64].bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    fs::copy(entry.path(), destination.join(name.as_ref())).unwrap();
                }
            }
        }
    }
    run(&one, &home, &["init"]);
    run(&one, &home, &["use", "node@24.1.0"]);
    let identity = generate_identity();
    let config_path = one.join("pinset.toml");
    let mut config = pinset_core::load_project_config(&config_path).unwrap();
    config.environment = Some(ProjectEnvironment {
        profiles: ["dev", "test"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    EnvironmentProfile {
                        file: format!("pinset.env/{name}.age"),
                        recipients: vec![identity.record.recipient.clone()],
                    },
                )
            })
            .collect(),
        ..Default::default()
    });
    pinset_core::save_project_config(&config_path, &config).unwrap();
    for name in ["dev", "test"] {
        write_encrypted_profile(
            &one,
            &format!("pinset.env/{name}.age"),
            &EnvironmentDocument {
                schema: 1,
                variables: BTreeMap::from([("WORKTREE_PROFILE".into(), name.into())]),
            },
            std::slice::from_ref(&identity.record.recipient),
        )
        .unwrap();
    }
    for arguments in [
        vec!["init", "--quiet"],
        vec!["add", "-f", "pinset.toml", "pinset.lock", "pinset.env"],
        vec![
            "-c",
            "user.name=Pinset fixture",
            "-c",
            "user.email=fixture@invalid.example",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        assert!(
            Command::new("git")
                .current_dir(&one)
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(
        Command::new("git")
            .current_dir(&one)
            .args(["worktree", "add", "--quiet", "-b", "acceptance-two"])
            .arg(&two)
            .status()
            .unwrap()
            .success()
    );
    run(&two, &home, &["use", "node@24.0.0"]);
    for (root, profile) in [(&one, "dev"), (&two, "test")] {
        run(root, &home, &["env", "use", profile]);
        run(root, &home, &["trust", "add"]);
    }
    let expression = "console.log(JSON.stringify({version:process.versions.node,profile:process.env.WORKTREE_PROFILE,path:process.execPath}))";
    std::thread::scope(|scope| {
        for (root, version, profile) in [(&one, "24.1.0", "dev"), (&two, "24.0.0", "test")] {
            let home = &home;
            let identity = &identity;
            scope.spawn(move || {
                let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
                    .current_dir(root)
                    .env("PINSET_HOME", home)
                    .env("PINSET_IDENTITY", identity.secret().expose_secret())
                    .args(["--", "node", "-e", expression])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["version"], version);
                assert_eq!(report["profile"], profile);
                assert!(report["path"].as_str().unwrap().contains(version));
            });
        }
    });
    // Removing a worktree must not affect the surviving worktree's SDK/profile/trust.
    assert!(
        Command::new("git")
            .current_dir(&one)
            .args(["worktree", "remove", "--force"])
            .arg(&two)
            .status()
            .unwrap()
            .success()
    );
    assert!(run(&one, &home, &["env"]).contains("trust=trusted"));
    assert!(run(&one, &home, &["--no-env", "--", "node", "--version"]).contains("24.1.0"));
}
