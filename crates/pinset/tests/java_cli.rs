use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

const JAVA_VERSION: &str = "21.0.8+9";

#[test]
fn routes_managed_jdk_commands_and_sets_java_home_without_shell_profile_changes() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        format!("schema = 2\n\n[tools]\njava = \"{JAVA_VERSION}\"\n"),
    )
    .expect("config");

    let install_dir = home
        .join("installs")
        .join("java")
        .join(JAVA_VERSION)
        .join(pinset_core::current_target_for_tool("java"));
    let java_home = if cfg!(target_os = "macos") {
        install_dir.join("Contents/Home")
    } else {
        install_dir.clone()
    };
    let bin = java_home.join("bin");
    fs::create_dir_all(&bin).expect("JDK bin");
    write_java_command(&bin, &java_home);
    write_javac_command(&bin);
    write_receipt(&install_dir);

    assert_success_contains(&pinset(&project, &home, &["list", "java"]), "java@21.0.8+9");
    assert_success_contains(
        &pinset(&project, &home, &["current", "java"]),
        "java 21.0.8+9 installed",
    );
    assert_success_contains(
        &pinset(&project, &home, &["which", "java"]),
        &bin.display().to_string(),
    );
    let executed = pinset(&project, &home, &["exec", "--", "java", "Hello"]);
    assert_success_contains(&executed, "fake-java Hello");
    assert_success_contains(
        &executed,
        &format!("JAVA_HOME={}", java_home.display()),
    );
    assert_success_contains(&executed, "CLASSPATH=fixture-classpath");
    assert_success_contains(
        &pinset(
            &project,
            &home,
            &["exec", "--", "javac", "Hello.java"],
        ),
        "fake-javac Hello.java",
    );
}

fn write_receipt(install_dir: &Path) {
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"java\"\nversion = \"{JAVA_VERSION}\"\ntarget = \"{}\"\n",
            pinset_core::current_target_for_tool("java")
        ),
    )
    .expect("receipt");
}

#[cfg(windows)]
fn write_java_command(directory: &Path, _java_home: &Path) {
    fs::write(
        directory.join("java.cmd"),
        "@echo off\r\necho fake-java %*\r\necho JAVA_HOME=%JAVA_HOME%\r\necho CLASSPATH=%CLASSPATH%\r\n",
    )
    .expect("java command");
}

#[cfg(unix)]
fn write_java_command(directory: &Path, _java_home: &Path) {
    write_unix_command(
        &directory.join("java"),
        "#!/bin/sh\nprintf 'fake-java %s\\nJAVA_HOME=%s\\nCLASSPATH=%s\\n' \"$*\" \"$JAVA_HOME\" \"$CLASSPATH\"\n",
    );
}

#[cfg(windows)]
fn write_javac_command(directory: &Path) {
    fs::write(
        directory.join("javac.cmd"),
        "@echo off\r\necho fake-javac %*\r\n",
    )
    .expect("javac command");
}

#[cfg(unix)]
fn write_javac_command(directory: &Path) {
    write_unix_command(
        &directory.join("javac"),
        "#!/bin/sh\nprintf 'fake-javac %s\\n' \"$*\"\n",
    );
}

#[cfg(unix)]
fn write_unix_command(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).expect("command");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
}

fn pinset(project: &Path, home: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(project)
        .env("PINSET_HOME", home)
        .env("PINSET_LANG", "en")
        .env("CLASSPATH", "fixture-classpath")
        .env_remove("JAVA_HOME")
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn assert_success_contains(output: &Output, expected: &str) {
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains(expected),
        "expected {expected:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
