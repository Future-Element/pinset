use pinset_core::{EnvironmentProfile, ProjectEnvironment};
use pinset_env::{EnvironmentDocument, generate_identity, trust_project, write_encrypted_profile};
use secrecy::ExposeSecret;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(root: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pinset"));
    cmd.current_dir(root)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .stdin(Stdio::null());
    for name in [
        "PINSET_ENV_PROFILE",
        "PINSET_ENV_DISABLE",
        "PINSET_IDENTITY",
        "PINSET_IDENTITY_FILE",
        "CI",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
    ] {
        cmd.env_remove(name);
    }
    cmd
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn print_variable(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.args(["--", "cmd.exe", "/d", "/c", "echo profile=%APP_TEST_VALUE%"]);
    #[cfg(not(windows))]
    cmd.args([
        "--",
        "sh",
        "-c",
        "printf 'profile=%s\\n' \"$APP_TEST_VALUE\"",
    ]);
}

#[test]
fn short_execution_preserves_child_arguments_exit_codes_and_legacy_errors() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let version = cli(root.path(), &home)
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_pinset"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(success(&version).contains(pinset_core::pinset_version()));
    let help = cli(root.path(), &home)
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_pinset"))
        .args(["--lang", "en", "--help"])
        .output()
        .unwrap();
    assert!(success(&help).contains("Usage:"));
    let mut exit = cli(root.path(), &home);
    #[cfg(windows)]
    exit.args(["--", "cmd.exe", "/d", "/c", "exit 37"]);
    #[cfg(not(windows))]
    exit.args(["--", "sh", "-c", "exit 37"]);
    assert_eq!(exit.output().unwrap().status.code(), Some(37));
    assert!(
        !cli(root.path(), &home)
            .args(["exec", "--", "unmanaged-command"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !cli(root.path(), &home)
            .args(["typo", "--", "cmd.exe"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !cli(root.path(), &home)
            .args(["-e", "dev", "--no-env", "--", "cmd.exe"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !home.exists(),
        "short read-only commands must not create local state"
    );
}

#[test]
fn local_profiles_apply_to_execution_and_broker_with_explicit_and_ci_overrides() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project with spaces");
    fs::create_dir(&project).unwrap();
    let config_path = pinset_core::create_project_config(&project).unwrap();
    let identity = generate_identity();
    let mut config = pinset_core::load_project_config(&config_path).unwrap();
    config.environment = Some(ProjectEnvironment {
        auto_profile: Some("dev".into()),
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
    let original = fs::read(&config_path).unwrap();
    for name in ["dev", "test"] {
        write_encrypted_profile(
            &project,
            &format!("pinset.env/{name}.age"),
            &EnvironmentDocument {
                schema: 1,
                variables: BTreeMap::from([("APP_TEST_VALUE".into(), name.into())]),
            },
            std::slice::from_ref(&identity.record.recipient),
        )
        .unwrap();
    }
    success(
        &cli(root.path(), &home)
            .arg("-C")
            .arg(&project)
            .args(["env", "use", "test"])
            .output()
            .unwrap(),
    );
    assert_eq!(fs::read(&config_path).unwrap(), original);
    assert!(
        success(&cli(&project, &home).arg("env").output().unwrap())
            .contains("profile=test source=local")
    );

    let mut untrusted = cli(&project, &home);
    untrusted.env("PINSET_IDENTITY", identity.secret().expose_secret());
    print_variable(&mut untrusted);
    assert!(!untrusted.output().unwrap().status.success());
    trust_project(
        &home,
        &project,
        config.project_id.as_deref().unwrap(),
        &toml::to_string(config.environment.as_ref().unwrap()).unwrap(),
    )
    .unwrap();

    for (flags, ci, process, expected) in [
        (vec![], false, None, "test"),
        (vec!["-e", "dev"], false, None, "dev"),
        (vec![], true, None, "dev"),
        (vec![], false, Some("dev"), "dev"),
        (vec!["-e", "test"], true, Some("dev"), "test"),
    ] {
        let mut command = cli(&project, &home);
        command
            .args(flags)
            .env("PINSET_IDENTITY", identity.secret().expose_secret());
        if ci {
            command.env("CI", "true");
        }
        if let Some(value) = process {
            command.env("PINSET_ENV_PROFILE", value);
        }
        print_variable(&mut command);
        assert!(success(&command.output().unwrap()).contains(&format!("profile={expected}")));
    }
    let broker = cli(&project, &home)
        .env("PINSET_IDENTITY", identity.secret().expose_secret())
        .args(["__env-resolve", "--cwd"])
        .arg(&project)
        .args(["--shim-version", pinset_core::pinset_version()])
        .output()
        .unwrap();
    assert!(broker.status.success());
    assert_eq!(
        pinset_core::decode_environment(&broker.stdout).unwrap()["APP_TEST_VALUE"],
        "test"
    );
    let colleague = generate_identity();
    success(
        &cli(&project, &home)
            .env("PINSET_IDENTITY", identity.secret().expose_secret())
            .args(["env", "share", &colleague.record.recipient])
            .output()
            .unwrap(),
    );
    assert!(
        success(
            &cli(&project, &home)
                .args(["env", "members"])
                .output()
                .unwrap()
        )
        .contains(&colleague.record.recipient)
    );
    assert!(
        pinset_env::read_encrypted_profile(
            &project,
            "pinset.env/test.age",
            std::slice::from_ref(colleague.secret())
        )
        .is_ok()
    );
    success(
        &cli(&project, &home)
            .env("PINSET_IDENTITY", identity.secret().expose_secret())
            .args(["env", "unshare", &colleague.record.recipient])
            .output()
            .unwrap(),
    );
    assert!(
        pinset_env::read_encrypted_profile(
            &project,
            "pinset.env/test.age",
            std::slice::from_ref(colleague.secret())
        )
        .is_err()
    );
    let mut disabled = cli(&project, &home);
    disabled.arg("--no-env").env("APP_TEST_VALUE", "inherited");
    print_variable(&mut disabled);
    assert!(success(&disabled.output().unwrap()).contains("profile=inherited"));

    // A copied project/worktree keeps the project-id but has an independent local selection.
    let worktree = root.path().join("other worktree");
    fs::create_dir(&worktree).unwrap();
    fs::copy(&config_path, worktree.join("pinset.toml")).unwrap();
    assert!(
        success(&cli(&worktree, &home).arg("env").output().unwrap())
            .contains("profile=dev source=project")
    );
    success(
        &cli(&worktree, &home)
            .args(["env", "use", "dev"])
            .output()
            .unwrap(),
    );
    assert!(
        success(&cli(&project, &home).arg("env").output().unwrap())
            .contains("profile=test source=local")
    );

    // Local selection also works with no shared auto-profile.
    config.environment.as_mut().unwrap().auto_profile = None;
    pinset_core::save_project_config(&config_path, &config).unwrap();
    assert!(
        success(&cli(&project, &home).arg("env").output().unwrap())
            .contains("profile=test source=local")
    );
    success(
        &cli(&project, &home)
            .args(["env", "reset"])
            .output()
            .unwrap(),
    );
    assert!(
        success(&cli(&project, &home).arg("env").output().unwrap())
            .contains("profile=none source=none")
    );
    config.environment.as_mut().unwrap().auto_profile = Some("dev".into());
    pinset_core::save_project_config(&config_path, &config).unwrap();
    success(
        &cli(&project, &home)
            .args(["env", "use", "--reset"])
            .output()
            .unwrap(),
    );
    assert!(
        success(&cli(&project, &home).arg("env").output().unwrap())
            .contains("profile=dev source=project")
    );
}

#[test]
fn external_execution_does_not_bypass_a_broken_managed_runtime() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::write(
        root.path().join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .unwrap();
    let output = cli(root.path(), &home)
        .arg("--")
        .arg(env!("CARGO_BIN_EXE_pinset"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!home.exists());
}

#[test]
#[ignore = "requires paired CLI/shim binaries; run by native CI after cargo build"]
fn paired_shim_uses_the_local_profile_without_a_shared_default() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project with spaces");
    let system = root.path().join("system");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&system).unwrap();
    let config_path = pinset_core::create_project_config(&project).unwrap();
    let identity = generate_identity();
    let mut config = pinset_core::load_project_config(&config_path).unwrap();
    config.policy.system_fallback = true;
    config.environment = Some(ProjectEnvironment {
        profiles: BTreeMap::from([(
            "dev".into(),
            EnvironmentProfile {
                file: "pinset.env/dev.age".into(),
                recipients: vec![identity.record.recipient.clone()],
            },
        )]),
        ..Default::default()
    });
    pinset_core::save_project_config(&config_path, &config).unwrap();
    write_encrypted_profile(
        &project,
        "pinset.env/dev.age",
        &EnvironmentDocument {
            schema: 1,
            variables: BTreeMap::from([("APP_TEST_VALUE".into(), "local-dev".into())]),
        },
        std::slice::from_ref(&identity.record.recipient),
    )
    .unwrap();
    success(
        &cli(&project, &home)
            .args(["env", "use", "dev"])
            .output()
            .unwrap(),
    );
    trust_project(
        &home,
        &project,
        config.project_id.as_deref().unwrap(),
        &toml::to_string(config.environment.as_ref().unwrap()).unwrap(),
    )
    .unwrap();
    #[cfg(windows)]
    fs::write(
        system.join("node.cmd"),
        "@echo off\r\necho %APP_TEST_VALUE%\r\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = system.join("node");
        fs::write(
            &path,
            "#!/bin/sh\nprintf '%s\\n' \"${APP_TEST_VALUE:-none}\"\n",
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let shim = std::env::var_os("PINSET_TEST_SHIM")
        .expect("PINSET_TEST_SHIM must point to the paired shim");
    for (ci, expected) in [(false, "local-dev"), (true, "none")] {
        let mut command = Command::new(&shim);
        command
            .current_dir(&project)
            .args(["--as", "node", "--cwd"])
            .arg(&project)
            .env("PINSET_HOME", &home)
            .env("PATH", &system)
            .env("APP_TEST_VALUE", "none")
            .env("PINSET_IDENTITY", identity.secret().expose_secret())
            .stdin(Stdio::null());
        for name in [
            "PINSET_ENV_PROFILE",
            "PINSET_ENV_DISABLE",
            "PINSET_IDENTITY_FILE",
            "CI",
            "GITHUB_ACTIONS",
            "GITLAB_CI",
            "TF_BUILD",
        ] {
            command.env_remove(name);
        }
        if ci {
            command.env("CI", "true");
        } else {
            command.env_remove("APP_TEST_VALUE");
        }
        assert!(success(&command.output().unwrap()).contains(expected));
    }
}

#[test]
fn incomplete_noninteractive_setup_has_no_side_effects() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config_path = pinset_core::create_project_config(root.path()).unwrap();
    let original = fs::read(&config_path).unwrap();
    for args in [vec!["env", "init"], vec!["env", "init", "dev"]] {
        let output = cli(root.path(), &home).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("non-interactive"));
    }
    assert_eq!(fs::read(config_path).unwrap(), original);
    assert!(!home.exists());
}
