use std::{fs, path::Path, process::Command};

use tempfile::tempdir;

#[test]
fn saves_chinese_and_uses_it_for_following_commands() {
    let root = tempdir().expect("temporary root");
    let home = root.path().join("home");
    let first_project = root.path().join("first");
    let override_project = root.path().join("override");
    let persisted_project = root.path().join("persisted");
    let environment_project = root.path().join("environment");
    fs::create_dir(&first_project).expect("first project");
    fs::create_dir(&override_project).expect("override project");
    fs::create_dir(&persisted_project).expect("persisted project");
    fs::create_dir(&environment_project).expect("environment project");

    let save = pinset(root.path(), &home, &["--lang", "zh-CN"]);
    assert_success_contains(&save, "语言已切换为中文");
    let settings = fs::read_to_string(home.join("settings.toml")).expect("settings");
    assert!(settings.contains("language = \"zh-CN\""));

    let chinese = pinset(&first_project, &home, &["init"]);
    assert_success_contains(&chinese, "已创建");

    let english_override = pinset(&override_project, &home, &["--lang", "en", "init"]);
    assert_success_contains(&english_override, "created");

    let still_chinese = pinset(&persisted_project, &home, &["init"]);
    assert_success_contains(&still_chinese, "已创建");

    let environment_override = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .arg("init")
        .current_dir(&environment_project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "en")
        .output()
        .expect("run with language environment override");
    assert_success_contains(&environment_override, "created");

    let localized_error = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "use", "invalid", "--no-install"],
    );
    assert!(!localized_error.status.success());
    let stderr = String::from_utf8_lossy(&localized_error.stderr);
    assert!(stderr.contains("错误："), "stderr: {stderr}");
    assert!(stderr.contains("版本选择必须使用"), "stderr: {stderr}");

    let help = pinset(&first_project, &home, &["--lang", "zh-CN", "use", "--help"]);
    assert_success_contains(&help, "选择并锁定 Node.js 版本");
    assert_success_contains(&help, "主版本|主次版本|lts|current");
    assert!(
        !String::from_utf8_lossy(&help.stdout).contains("Usage:"),
        "help should be localized: {}",
        String::from_utf8_lossy(&help.stdout)
    );

    let global_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "global", "--help"],
    );
    assert_success_contains(&global_help, "查看或设置项目之外使用的全局默认 Node.js");
    assert_success_contains(&global_help, "pinset global");

    let activate_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "activate", "--help"],
    );
    assert_success_contains(&activate_help, "Provider 命令路由");
    assert_success_contains(&activate_help, "pinset activate");

    let install_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "install", "--help"],
    );
    assert_success_contains(&install_help, "pinset install node@<版本选择器>");

    let import_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "import", "--help"],
    );
    assert_success_contains(&import_help, "pinset import --apply");

    let shim_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "shim", "--help"],
    );
    assert_success_contains(&shim_help, "pinset shim migrate");

    let unset_help = pinset(
        &first_project,
        &home,
        &["--lang", "zh-CN", "unset", "--help"],
    );
    assert_success_contains(&unset_help, "清除项目或全局 Node.js 选择");

    let argument_error = pinset(&first_project, &home, &["--lang", "zh-CN", "use"]);
    assert!(!argument_error.status.success());
    assert!(
        String::from_utf8_lossy(&argument_error.stderr).contains("错误：缺少必填参数"),
        "stderr: {}",
        String::from_utf8_lossy(&argument_error.stderr)
    );

    let core_error = pinset(&first_project, &home, &["--lang", "zh-CN", "which", "ruby"]);
    assert!(!core_error.status.success());
    assert!(
        String::from_utf8_lossy(&core_error.stderr).contains("当前不支持命令"),
        "stderr: {}",
        String::from_utf8_lossy(&core_error.stderr)
    );

    let sources = pinset(&first_project, &home, &["source", "list", "node"]);
    assert_success_contains(&sources, "状态=已启用");

    let persisted_help = pinset(&first_project, &home, &["--help"]);
    assert_success_contains(&persisted_help, "Pinset 用于统一管理");
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

fn assert_success_contains(output: &std::process::Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected success containing {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
