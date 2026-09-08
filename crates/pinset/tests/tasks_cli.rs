use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

use pinset_core::{ProjectConfig, ProjectPolicy, ProjectTask, save_project_config};
use tempfile::tempdir;

fn cli(root: &Path, home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pinset"));
    command
        .current_dir(root)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .stdin(Stdio::null());
    for name in [
        "PINSET_ENV_PROFILE",
        "PINSET_ENV_DISABLE",
        "CI",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TF_BUILD",
    ] {
        command.env_remove(name);
    }
    command
}

fn success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).expect("UTF-8 output")
}

fn task(command: impl IntoIterator<Item = impl AsRef<OsStr>>) -> ProjectTask {
    ProjectTask {
        command: command
            .into_iter()
            .map(|value| value.as_ref().to_string_lossy().into_owned())
            .collect(),
        cwd: None,
        profile: None,
        description: None,
        python_environment: None,
    }
}

#[test]
fn named_tasks_preserve_arguments_cwd_and_exit_status_without_path_fallback() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let package = project.join("package");
    fs::create_dir_all(&package).expect("task directory");

    #[cfg(windows)]
    let argument_task = task([
        "powershell.exe",
        "-NoProfile",
        "-Command",
        "& { param([string]$value) Write-Output ('arg=' + $value) }",
    ]);
    #[cfg(not(windows))]
    let argument_task = task(["sh", "-c", "printf 'arg=%s\\n' \"$1\"", "probe"]);

    #[cfg(windows)]
    let exit_task = task(["cmd.exe", "/d", "/c", "exit 31"]);
    #[cfg(not(windows))]
    let exit_task = task(["sh", "-c", "exit 31"]);

    #[cfg(windows)]
    let mut cwd_task = task(["cmd.exe", "/d", "/c", "cd"]);
    #[cfg(not(windows))]
    let mut cwd_task = task(["pwd"]);
    cwd_task.cwd = Some("package".to_owned());

    let config = ProjectConfig {
        schema: 5,
        project_id: Some("4c5652e4-0000-4000-8000-000000000001".to_owned()),
        policy: ProjectPolicy {
            system_fallback: true,
            ..ProjectPolicy::default()
        },
        tools: BTreeMap::new(),
        tool_options: Default::default(),
        tasks: BTreeMap::from([
            ("args".to_owned(), argument_task),
            ("cwd".to_owned(), cwd_task),
            ("exit".to_owned(), exit_task),
            (
                "version".to_owned(),
                task([env!("CARGO_BIN_EXE_pinset"), "--version"]),
            ),
        ]),
        python: None,
        workspace: None,
        environment: None,
    };
    save_project_config(&project.join("pinset.toml"), &config).expect("project config");

    let version = cli(&project, &home)
        .args(["run", "version"])
        .output()
        .expect("run version task");
    assert!(success(&version).contains(pinset_core::pinset_version()));

    let arguments = cli(&project, &home)
        .args(["run", "args", "--", "watch"])
        .output()
        .expect("run task with appended argument");
    assert!(success(&arguments).contains("arg=watch"));

    let cwd = cli(&project, &home)
        .args(["run", "cwd"])
        .output()
        .expect("run task cwd");
    let printed_cwd = success(&cwd);
    assert!(
        printed_cwd
            .trim()
            .eq_ignore_ascii_case(&package.to_string_lossy())
    );

    let exit = cli(&project, &home)
        .args(["run", "exit"])
        .output()
        .expect("run failing task");
    assert_eq!(exit.status.code(), Some(31));

    let unknown = cli(&project, &home)
        .args(["run", "pinset"])
        .output()
        .expect("run unknown task");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("is not declared"));
}

#[test]
fn task_cwd_cannot_escape_the_project() {
    let temporary = tempdir().expect("temporary root");
    let project = temporary.path().join("project");
    fs::create_dir(&project).expect("project");
    let config = format!(
        "schema = 5\nproject-id = \"4c5652e4-0000-4000-8000-000000000002\"\n\n[tasks.bad]\ncommand = [\"{}\", \"--version\"]\ncwd = \"../\"\n\n[tools]\n",
        env!("CARGO_BIN_EXE_pinset").replace('\\', "\\\\")
    );
    fs::write(project.join("pinset.toml"), config).expect("invalid project config");

    let output = cli(&project, &temporary.path().join("home"))
        .args(["run", "bad"])
        .output()
        .expect("run invalid task");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cwd must stay within the project"));
}
