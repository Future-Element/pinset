use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

const PROVIDER_VERSIONS: &[(&str, &str)] = &[
    ("node", "24.1.0"),
    ("pnpm", "11.21.0"),
    ("bun", "1.3.14"),
    ("go", "1.25.1"),
    ("flutter", "3.47.0"),
    ("python", "3.14.7+20260807"),
    ("java", "21.0.8+9"),
    ("rust", "1.97.0"),
    ("dotnet", "10.0.100"),
];

#[test]
fn every_provider_supports_the_shared_local_lifecycle_contract() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let project = root.path().join("project");
    fs::create_dir_all(&project).expect("project");
    write_global_config(&home);

    for provider in pinset_core::runtime_providers() {
        let version = version_for(provider.tool);
        create_install(
            &home,
            provider.tool,
            version,
            provider.commands.iter().copied(),
        );

        let listed = pinset(&project, &home, &["list", provider.tool, "--json"]);
        assert_success(&listed);
        let listed: serde_json::Value =
            serde_json::from_slice(&listed.stdout).expect("installed JSON");
        assert_eq!(listed[0]["tool"], provider.tool);
        assert_eq!(listed[0]["version"], version);

        let current = pinset(&project, &home, &["current", provider.tool, "--json"]);
        assert_success(&current);
        let current: serde_json::Value =
            serde_json::from_slice(&current.stdout).expect("current JSON");
        assert_eq!(current["tool"], provider.tool);
        assert_eq!(current["version"], version);
        assert_eq!(current["source"], "global");
        assert_eq!(current["installed"].as_bool(), Some(true));

        for command in provider.commands {
            let resolved = pinset(&project, &home, &["which", command, "--json"]);
            assert_success(&resolved);
            let resolved: serde_json::Value =
                serde_json::from_slice(&resolved.stdout).expect("which JSON");
            assert_eq!(resolved["command"], *command);
            assert_eq!(resolved["tool"], provider.tool);
            assert_eq!(resolved["version"], version);
            assert_eq!(resolved["source"], "global");
        }
    }
}

fn write_global_config(home: &Path) {
    let state = home.join("state");
    fs::create_dir_all(&state).expect("global state");
    let mut config = String::from("schema = 2\n\n[tools]\n");
    for (tool, version) in PROVIDER_VERSIONS {
        config.push_str(&format!("{tool} = \"{version}\"\n"));
    }
    fs::write(state.join("global.toml"), config).expect("global config");
}

fn version_for(tool: &str) -> &'static str {
    PROVIDER_VERSIONS
        .iter()
        .find_map(|(provider, version)| (*provider == tool).then_some(*version))
        .unwrap_or_else(|| panic!("missing fixture version for provider {tool}"))
}

fn create_install<'a>(
    home: &Path,
    tool: &str,
    version: &str,
    commands: impl Iterator<Item = &'a str>,
) {
    let target = pinset_core::current_target_for_tool(tool);
    let root = home.join("installs").join(tool).join(version).join(&target);
    let command_directory = pinset_core::runtime_command_directory(tool, &root);
    fs::create_dir_all(&command_directory).expect("command directory");
    fs::write(
        root.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"{tool}\"\nversion = \"{version}\"\ntarget = \"{target}\"\n"
        ),
    )
    .expect("receipt");
    for command in commands {
        write_command(&command_directory, command);
    }
}

#[cfg(windows)]
fn write_command(directory: &Path, command: &str) {
    fs::write(directory.join(format!("{command}.cmd")), "@echo off\r\n").expect("command");
}

#[cfg(unix)]
fn write_command(directory: &Path, command: &str) {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(command);
    fs::write(&path, "#!/bin/sh\n").expect("command");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
