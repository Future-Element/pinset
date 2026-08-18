use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use tempfile::tempdir;

#[test]
fn resolves_lists_and_executes_a_managed_flutter_sdk_with_its_bundled_dart() {
    let root = tempdir().expect("root");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n\n[tools]\nflutter = \"3.47.0\"\n",
    )
    .expect("config");

    let install_dir = home
        .join("installs")
        .join("flutter")
        .join("3.47.0")
        .join(pinset_core::current_target_for_tool("flutter"));
    let bin_dir = install_dir.join("bin");
    fs::create_dir_all(&bin_dir).expect("Flutter bin");
    write_flutter_command(&bin_dir);
    write_dart_command(&bin_dir);
    fs::write(
        install_dir.join(".pinset-install.toml"),
        format!(
            "schema = 2\ncomplete = true\ntool = \"flutter\"\nversion = \"3.47.0\"\ntarget = \"{}\"\n",
            pinset_core::current_target_for_tool("flutter")
        ),
    )
    .expect("receipt");

    assert_success_contains(
        &pinset(&project, &home, &["list", "flutter"]),
        "flutter@3.47.0",
    );
    assert_success_contains(
        &pinset(&project, &home, &["current", "flutter"]),
        "flutter 3.47.0 installed",
    );
    assert_success_contains(
        &pinset(&project, &home, &["which", "dart"]),
        &bin_dir.display().to_string(),
    );

    let flutter = pinset(&project, &home, &["exec", "--", "flutter", "--version"]);
    assert_success_contains(&flutter, "fake-flutter-3.47.0 --version");
    assert_success_contains(&flutter, &format!("FLUTTER_ROOT={}", install_dir.display()));
    assert_success_contains(&flutter, "FLUTTER_SUPPRESS_ANALYTICS=true");

    let dart = pinset(&project, &home, &["exec", "--", "dart", "--version"]);
    assert_success_contains(&dart, "fake-dart-3.13.0 --version");
    assert_success_contains(&dart, &format!("FLUTTER_ROOT={}", install_dir.display()));

    let explicit_policy = Command::new(env!("CARGO_BIN_EXE_pinset"))
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .env("PINSET_LANG", "en")
        .env("FLUTTER_SUPPRESS_ANALYTICS", "false")
        .args(["exec", "--", "flutter", "doctor"])
        .output()
        .expect("run with explicit analytics policy");
    assert_success_contains(&explicit_policy, "FLUTTER_SUPPRESS_ANALYTICS=false");

    let mutation = pinset(&project, &home, &["exec", "--", "flutter", "upgrade"]);
    assert!(!mutation.status.success());
    assert!(
        String::from_utf8_lossy(&mutation.stderr)
            .contains("refusing to run `flutter upgrade` against a Pinset-managed Flutter SDK"),
        "stderr: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    assert!(
        mutation.stdout.is_empty(),
        "managed Flutter must not be invoked"
    );

    assert_success_contains(
        &pinset(&project, &home, &["exec", "--", "flutter", "pub", "get"]),
        "fake-flutter-3.47.0 pub get",
    );
}

#[cfg(windows)]
fn write_flutter_command(directory: &Path) {
    fs::write(
        directory.join("flutter.cmd"),
        "@echo off\r\necho fake-flutter-3.47.0 %*\r\necho FLUTTER_ROOT=%FLUTTER_ROOT%\r\necho FLUTTER_SUPPRESS_ANALYTICS=%FLUTTER_SUPPRESS_ANALYTICS%\r\n",
    )
    .expect("flutter command");
}

#[cfg(unix)]
fn write_flutter_command(directory: &Path) {
    write_unix_command(
        &directory.join("flutter"),
        "#!/bin/sh\nprintf 'fake-flutter-3.47.0 %s\\nFLUTTER_ROOT=%s\\nFLUTTER_SUPPRESS_ANALYTICS=%s\\n' \"$*\" \"$FLUTTER_ROOT\" \"$FLUTTER_SUPPRESS_ANALYTICS\"\n",
    );
}

#[cfg(windows)]
fn write_dart_command(directory: &Path) {
    fs::write(
        directory.join("dart.cmd"),
        "@echo off\r\necho fake-dart-3.13.0 %*\r\necho FLUTTER_ROOT=%FLUTTER_ROOT%\r\n",
    )
    .expect("dart command");
}

#[cfg(unix)]
fn write_dart_command(directory: &Path) {
    write_unix_command(
        &directory.join("dart"),
        "#!/bin/sh\nprintf 'fake-dart-3.13.0 %s\\nFLUTTER_ROOT=%s\\n' \"$*\" \"$FLUTTER_ROOT\"\n",
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
        .env_remove("FLUTTER_ROOT")
        .env_remove("FLUTTER_SUPPRESS_ANALYTICS")
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
