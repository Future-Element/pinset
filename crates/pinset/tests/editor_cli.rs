use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn editor_context_is_secret_free_versioned_and_folder_scoped() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let first = root.path().join("first");
    let second = root.path().join("second");
    fs::create_dir(&first).expect("first project");
    fs::create_dir(&second).expect("second project");
    write_project(&first, "11111111-1111-4111-8111-111111111111", "dev");
    write_project(&second, "22222222-2222-4222-8222-222222222222", "test");

    let first_context = context(&first, &home);
    let second_context = context(&second, &home);
    for value in [&first_context, &second_context] {
        assert_eq!(value["schema"], 1);
        assert_eq!(value["command"], "editor.context");
        assert_eq!(value["data"]["protocol_schema"], 1);
        assert_eq!(value["data"]["minimum_extension_version"], "1.0.0");
        assert_eq!(value["data"]["requires_workspace_trust"], true);
        assert_eq!(value["data"]["tasks"][1]["name"], "test");
        assert_eq!(value["data"]["tasks"][1]["depends_on"][0], "setup");
        assert!(value.to_string().find("DATABASE_PASSWORD").is_none());
        assert!(value.to_string().find("secret-value").is_none());
        assert!(
            value
                .to_string()
                .find("private-task-command-argument")
                .is_none()
        );
    }
    assert_eq!(first_context["data"]["environment"]["selected"], "dev");
    assert_eq!(second_context["data"]["environment"]["selected"], "test");
    assert_ne!(
        first_context["data"]["project_root"],
        second_context["data"]["project_root"]
    );
}

fn context(project: &Path, home: &Path) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .env("PINSET_HOME", home)
        .env_remove("PINSET_ENV_PROFILE")
        .args([
            "editor",
            "context",
            "--cwd",
            project.to_str().expect("project path"),
            "--json",
        ])
        .output()
        .expect("run editor context");
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("editor context JSON")
}

fn write_project(project: &Path, project_id: &str, profile: &str) {
    fs::write(
        project.join("pinset.toml"),
        format!(
            r#"schema = 5
project-id = "{project_id}"

[tools]

[tasks.setup]
command = ["pinset", "private-task-command-argument"]
description = "Prepare the project"

[tasks.test]
command = ["pinset", "--version"]
depends-on = ["setup"]
profile = "{profile}"

[environment]
auto-profile = "{profile}"

[environment.profiles.{profile}]
file = ".pinset/{profile}.age"
recipients = ["age1example"]

[environment.variables.DATABASE_PASSWORD]
type = "string"
required = true
secret = true
profiles = ["{profile}"]
"#
        ),
    )
    .expect("project config");
}
