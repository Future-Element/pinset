use pinset_core::*;
use std::collections::BTreeMap;

fn definition() -> GenericTool {
    toml::from_str(
        r#"
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
"#,
    )
    .unwrap()
}

fn tool() -> LockedTool {
    LockedTool {
        name: "terraform".into(),
        requested: "1.9.0".into(),
        version: "1.9.0".into(),
        provider: GENERIC_PROVIDER.into(),
        released_at: None,
        metadata: BTreeMap::new(),
        options: BTreeMap::from([("executable:linux-x86_64".into(), "terraform".into())]),
        artifacts: vec![LockedArtifact {
            target: "linux-x86_64".into(),
            canonical_url: definition().artifact_url("1.9.0", "linux-x86_64").unwrap(),
            artifact_path: "terraform_1.9.0_linux_amd64.zip".into(),
            sha256: "a".repeat(64),
            integrity: None,
            format: LockedArtifactFormat::Zip,
            archive_root: String::new(),
            verification: GENERIC_VERIFICATION.into(),
            attestation: None,
            overlays: vec![],
        }],
    }
}

#[test]
fn attestation_claims_require_matching_evidence_and_cannot_downgrade() {
    let unsigned = tool();
    let mut signed = unsigned.clone();
    let artifact = &mut signed.artifacts[0];
    artifact.canonical_url =
        "https://github.com/owner/terraform/releases/download/v1.9.0/tool.zip".into();
    artifact.verification = GENERIC_GITHUB_VERIFICATION.into();
    assert!(validate_generic_lock(&signed).is_err());
    signed.artifacts[0].attestation = Some(GitHubAttestation {
        repository: "owner/terraform".into(),
        bundle: "{}".into(),
    });
    // Core validates the evidence contract only; engine must verify the bundle.
    validate_generic_lock(&signed).unwrap();
    assert_eq!(
        tool_verification_strength(&signed).unwrap(),
        VerificationStrength::SignedChecksum
    );
    assert!(validate_verification_transition(&signed, &unsigned).is_err());
    let mut mixed = signed.clone();
    let mut second = unsigned.artifacts[0].clone();
    second.target = "windows-x86_64".into();
    mixed.artifacts.push(second);
    mixed
        .options
        .insert("executable:windows-x86_64".into(), "terraform.exe".into());
    let mut weakened = mixed.clone();
    weakened.artifacts[0].attestation = None;
    weakened.artifacts[0].verification = GENERIC_VERIFICATION.into();
    validate_generic_lock(&weakened).unwrap();
    assert!(validate_verification_transition(&mixed, &weakened).is_err());
    signed.artifacts[0].attestation.as_mut().unwrap().repository = "other/terraform".into();
    assert!(validate_generic_lock(&signed).is_err());
    signed.artifacts[0].attestation.as_mut().unwrap().repository = "owner/terraform".into();
    signed.artifacts[0].verification = GENERIC_VERIFICATION.into();
    assert!(validate_generic_lock(&signed).is_err());
}

#[test]
fn release_identity_cannot_be_borrowed_from_other_download_hosts_or_paths() {
    assert_eq!(
        github_release_repository("https://github.com/owner/tool/releases/download/v1/tool.zip"),
        Some("owner/tool".into())
    );
    for url in [
        "https://github.com.evil.example/owner/tool/releases/download/v1/tool.zip",
        "https://github.com/owner/tool/archive/refs/tags/v1.zip",
        "https://github.com/owner%2Fother/tool/releases/download/v1/tool.zip",
        "https://github.com/owner/tool/releases/download/v1/tool.zip/extra",
        "https://github.com:444/owner/tool/releases/download/v1/tool.zip",
        "https://release-assets.githubusercontent.com/path/tool.zip",
    ] {
        assert!(github_release_repository(url).is_none(), "{url}");
    }
}

#[test]
fn configuration_preserves_string_selections_and_renders_every_platform() {
    let mut config = ProjectConfig::new("generic-project".into());
    config.generic.insert("terraform".into(), definition());
    config.tools.insert("terraform".into(), "1.9.0".into());
    config.validate().unwrap();
    let text = toml::to_string_pretty(&config).unwrap();
    assert!(text.contains("[generic.terraform]"));
    assert_eq!(toml::from_str::<ProjectConfig>(&text).unwrap(), config);
    for (target, filename) in [
        ("linux-x86_64", "linux_amd64"),
        ("linux-aarch64", "linux_arm64"),
        ("macos-x86_64", "darwin_amd64"),
        ("macos-aarch64", "darwin_arm64"),
        ("windows-x86_64", "windows_amd64"),
    ] {
        assert!(
            definition()
                .artifact_url("1.9.0", target)
                .unwrap()
                .ends_with(&format!("{filename}.zip"))
        );
        assert_eq!(
            definition().executable_path("1.9.0", target).unwrap(),
            if target.starts_with("windows") {
                "terraform.exe"
            } else {
                "terraform"
            }
        );
    }
}

#[test]
fn generic_names_cannot_claim_builtin_commands_or_unsafe_entries() {
    for name in [
        "node",
        "npm",
        "java",
        "javac",
        "rust",
        "cargo",
        "pinset",
        "pinset-shim",
        "../tool",
        "tool.exe",
        "MyTool",
        "nul",
        "com1",
        "-tool",
    ] {
        assert!(validate_generic_name(name).is_err(), "{name}");
    }
    for name in ["terraform", "jq", "git-lfs", "my_cli"] {
        validate_generic_name(name).unwrap();
    }
    for version in [
        "latest",
        "lts",
        "stable",
        ">=1",
        "^1.9",
        "1.*",
        "../version",
        "",
        "1 2",
    ] {
        assert!(validate_generic_version(version).is_err(), "{version}");
    }
    for version in ["1.9.0", "v1.9.0", "2026-10-22", "1.0.0-rc.1+build"] {
        validate_generic_version(version).unwrap();
    }
}

#[test]
fn only_safe_https_templates_and_relative_executable_paths_are_accepted() {
    for url in [
        "http://example.com/tool",
        "file:///tmp/tool",
        "https://user:secret@example.com/tool",
        "https://127.0.0.1/tool",
        "https://[::1]/tool",
        "https://10.0.0.1/tool",
        "https://localhost./tool",
        "https://example.local/tool",
        "https://example.com/tool#fragment",
    ] {
        assert!(validate_generic_url(url).is_err(), "{url}");
    }
    for url in [
        "https://github.com/owner/tool/releases/download/v1/tool.zip",
        "https://release-assets.githubusercontent.com/path?signature=public-fixture",
    ] {
        validate_generic_url(url).unwrap();
    }
    for path in [
        "../tool",
        "/tool",
        "bin/../../tool",
        "C:/tool.exe",
        "bin\\tool",
        "bin//tool",
        "bin/./tool",
        "",
        "bin/\n",
    ] {
        assert!(validate_generic_path(path).is_err(), "{path:?}");
    }
    for path in ["tool", "package/bin/tool", "package/bin/tool.exe"] {
        validate_generic_path(path).unwrap();
    }
    let mut value = definition();
    value.url = "https://example.com/{unknown}".into();
    assert!(value.validate("terraform").is_err());
    value = definition();
    value.format = LockedArtifactFormat::TarXz;
    assert!(value.validate("terraform").is_err());
}

#[test]
fn generic_locks_require_exact_versions_safe_paths_and_sha256() {
    let mut lock = Lockfile::empty("generic-project".into());
    lock.upsert_tool(tool()).unwrap();
    lock.validate().unwrap();
    for mutate in [
        |t: &mut LockedTool| t.artifacts[0].sha256 = "bad".into(),
        |t: &mut LockedTool| t.artifacts[0].canonical_url = "http://example.com/tool".into(),
        |t: &mut LockedTool| {
            t.options
                .insert("executable:linux-x86_64".into(), "../outside".into());
        },
        |t: &mut LockedTool| t.artifacts[0].format = LockedArtifactFormat::TarXz,
        |t: &mut LockedTool| t.requested = "latest".into(),
        |t: &mut LockedTool| t.options.clear(),
    ] {
        let mut changed = tool();
        mutate(&mut changed);
        assert!(validate_generic_lock(&changed).is_err());
    }
    let mut masquerading = tool();
    masquerading.name = "node".into();
    assert!(lock.upsert_tool(masquerading.clone()).is_err());
    masquerading.provider = "nodejs-official".into();
    lock.tools = vec![masquerading];
    assert!(
        lock.validate().is_err(),
        "built-in official URL checks must remain enforced"
    );
}

#[test]
fn installation_lock_is_independent_of_later_template_edits() {
    let mut config = ProjectConfig::new("generic-project".into());
    config.tools.insert("terraform".into(), "1.9.0".into());
    config.generic.insert("terraform".into(), definition());
    let mut lock = Lockfile::empty(config.project_id.clone());
    lock.upsert_tool(tool()).unwrap();
    config.validate_lock(&lock).unwrap();
    config.generic.get_mut("terraform").unwrap().url =
        "https://different.example.com/{version}/tool.zip".into();
    config.generic.get_mut("terraform").unwrap().executable = "new/location/terraform{exe}".into();
    config.validate_lock(&lock).unwrap();
    assert_eq!(
        generic_executable(lock.tool("terraform").unwrap(), "linux-x86_64").unwrap(),
        "terraform"
    );
    config.tools.insert("terraform".into(), "1.9.1".into());
    assert_eq!(
        config.validate_lock(&lock).unwrap_err().code(),
        "PINSET_LOCK_MISMATCH"
    );
}

#[test]
fn unsigned_download_hashes_do_not_satisfy_a_signed_checksum_policy() {
    let mut policy = ProjectPolicy {
        minimum_verification: Some("checksum".into()),
        ..ProjectPolicy::default()
    };
    policy
        .validate_tool(&tool(), std::time::SystemTime::now())
        .unwrap();
    policy.minimum_verification = Some("signed-checksum".into());
    assert!(
        policy
            .validate_tool(&tool(), std::time::SystemTime::now())
            .is_err()
    );
    policy.minimum_verification = None;
    policy.minimum_release_age = Some("7d".into());
    assert!(
        policy
            .validate_tool(&tool(), std::time::SystemTime::now())
            .is_err()
    );
    validate_registered_command("terraform", "terraform").unwrap();
    assert!(validate_registered_command("other", "terraform").is_err());
    assert!(validate_registered_command("npm", "terraform").is_err());
}
