use std::{fs, process::Command};

use tempfile::tempdir;

#[test]
fn init_creates_a_minimal_config_without_runtime_side_effects() {
    let root = tempdir().expect("temporary project");
    let project = root.path().join("project");
    let isolated_home = root.path().join("isolated-home");
    fs::create_dir(&project).expect("project directory");

    let output = pinset_init(&project, &isolated_home);

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config_path = project.join("pinset.toml");
    assert_eq!(
        fs::read_to_string(&config_path).expect("created config"),
        "schema = 1\n\n[tools]\n"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&config_path.display().to_string()));
    assert!(!isolated_home.exists());
    assert_eq!(
        fs::read_dir(&project).expect("project directory").count(),
        1
    );
}

#[test]
fn init_refuses_to_overwrite_an_existing_config() {
    let root = tempdir().expect("temporary project");
    let project = root.path().join("project");
    let isolated_home = root.path().join("isolated-home");
    fs::create_dir(&project).expect("project directory");
    let config_path = project.join("pinset.toml");
    let original = "schema = 1\n\n[tools]\nnode = \"24\"\n";
    fs::write(&config_path, original).expect("existing config");

    let output = pinset_init(&project, &isolated_home);

    assert!(!output.status.success(), "existing config must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing to overwrite"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(config_path).expect("existing config"),
        original
    );
    assert!(!isolated_home.exists());
}

fn pinset_init(project: &std::path::Path, isolated_home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .arg("init")
        .current_dir(project)
        .env("PINSET_HOME", isolated_home)
        .output()
        .expect("run pinset init")
}
