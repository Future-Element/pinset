use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use pinset_core::{current_target, install_shims};
use tempfile::tempdir;

#[test]
fn executes_fake_node_selected_by_nearest_project_config() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let nested = project.join("packages").join("app").join("src");
    let home = root.path().join("home");
    fs::create_dir_all(&nested).expect("nested directory");
    fs::create_dir(project.join(".git")).expect("git marker");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&nested)
        .args(["--", "hello", "pinset"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("20.0.0:hello pinset"), "stdout: {stdout}");
    assert!(stdout.contains("source=project"), "stdout: {stdout}");
}

#[cfg(windows)]
#[test]
fn forwards_cmd_arguments_without_interpreting_shell_metacharacters() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project directory");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "safe&echo PINSET_ARGUMENT_INJECTION"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("cmd.exe metacharacters cannot be forwarded safely")
    );
}

#[test]
fn executes_fake_node_selected_by_global_config_without_a_project() {
    let root = tempdir().expect("temp directory");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    fs::create_dir_all(&workspace).expect("workspace");
    fs::create_dir_all(home.join("state")).expect("global state");
    fs::write(
        home.join("state").join("global.toml"),
        "schema = 1\n[tools]\nnode = \"24.0.0\"\n",
    )
    .expect("global config");
    create_fake_node(&home, "24.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&workspace)
        .args(["--", "hello", "global"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("24.0.0:hello global"), "stdout: {stdout}");
    assert!(stdout.contains("source=global"), "stdout: {stdout}");
}

#[test]
fn rejects_a_command_already_present_in_the_shim_chain() {
    let shim = PathBuf::from(env!("CARGO_BIN_EXE_pinset-shim"));
    let owner = shim
        .canonicalize()
        .unwrap_or_else(|_| shim.clone())
        .to_string_lossy()
        .into_owned();
    let owner = if cfg!(windows) {
        owner.to_ascii_lowercase()
    } else {
        owner
    };
    let output = Command::new(&shim)
        .args(["--as", "node"])
        .env("PINSET_SHIM_CHAIN", "pnpm,node")
        .env("PINSET_SHIM_OWNER", owner)
        .output()
        .expect("run shim");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("recursive shim invocation detected: pnpm -> node -> node"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ignores_a_shim_chain_owned_by_another_router() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "foreign-chain"])
        .env("PINSET_HOME", &home)
        .env("PINSET_SHIM_CHAIN", "node")
        .env("PINSET_SHIM_OWNER", "another-pinset-router")
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("20.0.0:foreign-chain"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn executes_another_selected_provider_from_a_managed_pnpm_process() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nbun = \"1.3.14\"\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "24.0.0");
    create_fake_pnpm(&home, "11.22.0");
    create_fake_bun(&home, "1.3.14");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "pnpm", "--cwd"])
        .arg(&project)
        .args(["--", "dev"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run pnpm shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bun:dev"), "stdout: {stdout}");
    assert!(stdout.contains("chain=pnpm"), "stdout: {stdout}");
}

#[test]
fn executes_a_three_provider_chain_without_reentering_shims() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nbun = \"1.3.14\"\ngo = \"1.25.1\"\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "24.0.0");
    create_fake_pnpm(&home, "11.22.0");
    create_fake_bun(&home, "1.3.14");
    create_fake_go(&home, "1.25.1");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "pnpm", "--cwd"])
        .arg(&project)
        .args(["--", "chain3"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run provider chain");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("go:final"), "stdout: {stdout}");
    assert!(stdout.contains("chain=pnpm"), "stdout: {stdout}");
}

#[test]
fn executes_the_same_managed_provider_in_a_child_process_without_reentering_the_shim() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "spawn-node"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run same-provider child");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("node-child"), "stdout: {stdout}");
    assert!(stdout.contains("chain=node"), "stdout: {stdout}");
}

#[test]
fn executes_a_managed_system_managed_chain_with_the_effective_provider_path() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let system_bin = root.path().join("system-bin");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nbun = \"1.3.14\"\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");
    create_fake_bun(&home, "1.3.14");
    create_fake_system_bridge(&system_bin);
    let inherited_path = env::var_os("PATH");
    let path = std::iter::once(system_bin).chain(
        inherited_path
            .as_ref()
            .into_iter()
            .flat_map(|value| env::split_paths(value)),
    );
    let path = env::join_paths(path).expect("test PATH");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "bridge"])
        .env("PINSET_HOME", &home)
        .env("PATH", path)
        .output()
        .expect("run mixed provider chain");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("bun:bridged"), "stdout: {stdout}");
    assert!(stdout.contains("chain=node"), "stdout: {stdout}");
}

#[test]
fn executes_an_inherited_global_provider_from_a_managed_pnpm_process() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::create_dir_all(home.join("state")).expect("global state");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[policy]\ninherit-global = true\n[tools]\nnode = \"24.0.0\"\npnpm = \"11.22.0\"\n",
    )
    .expect("project config");
    fs::write(
        home.join("state/global.toml"),
        "schema = 1\n[tools]\nbun = \"1.3.14\"\n",
    )
    .expect("global config");
    create_fake_node(&home, "24.0.0");
    create_fake_pnpm(&home, "11.22.0");
    create_fake_bun(&home, "1.3.14");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "pnpm", "--cwd"])
        .arg(&project)
        .args(["--", "dev"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run pnpm shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("bun:dev"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn preserves_the_runtime_exit_code() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"20.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "20.0.0");

    let status = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "node", "--cwd"])
        .arg(&project)
        .args(["--", "exit42"])
        .env("PINSET_HOME", &home)
        .status()
        .expect("run shim");

    assert_eq!(status.code(), Some(42));
}

#[test]
fn executes_through_an_installed_multicall_shim_name() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    let shims = home.join("shims");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 1\n[tools]\nnode = \"22.0.0\"\n",
    )
    .expect("project config");
    create_fake_node(&home, "22.0.0");

    let installed = install_shims(
        Path::new(env!("CARGO_BIN_EXE_pinset-shim")),
        &shims,
        &["node".to_owned()],
    )
    .expect("install shim");
    let node_shim = &installed[0].destination;
    let output = Command::new(node_shim)
        .arg("multicall")
        .current_dir(&project)
        .env("PINSET_HOME", &home)
        .output()
        .expect("run installed shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("22.0.0:multicall"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn installed_shim_passes_through_to_system_path_without_recursing() {
    let root = tempdir().expect("temp directory");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    let shims = home.join("shims");
    let system_bin = root.path().join("system-bin");
    fs::create_dir_all(&workspace).expect("workspace");
    create_fake_system_node(&system_bin);
    let installed = install_shims(
        Path::new(env!("CARGO_BIN_EXE_pinset-shim")),
        &shims,
        &["node".to_owned()],
    )
    .expect("install shim");
    let inherited_path = env::var_os("PATH");
    let path = std::iter::once(shims.clone())
        .chain(std::iter::once(system_bin.clone()))
        .chain(
            inherited_path
                .as_ref()
                .into_iter()
                .flat_map(|value| env::split_paths(value)),
        );
    let path = env::join_paths(path).expect("test PATH");

    let output = Command::new(&installed[0].destination)
        .arg("passthrough")
        .current_dir(&workspace)
        .env("PINSET_HOME", &home)
        .env("PATH", path)
        .output()
        .expect("run installed shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("system:passthrough"), "stdout: {stdout}");
    assert!(stdout.contains("source=system"), "stdout: {stdout}");
}

#[test]
fn installed_shim_passes_through_a_system_provider_with_declared_dependencies() {
    let root = tempdir().expect("temp directory");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    let shims = home.join("shims");
    let system_bin = root.path().join("system-bin");
    fs::create_dir_all(&workspace).expect("workspace");
    create_fake_system_pnpm_and_node(&system_bin);
    let installed = install_shims(
        Path::new(env!("CARGO_BIN_EXE_pinset-shim")),
        &shims,
        &["pnpm".to_owned()],
    )
    .expect("install pnpm shim");
    let inherited_path = env::var_os("PATH");
    let path = std::iter::once(shims)
        .chain(std::iter::once(system_bin))
        .chain(
            inherited_path
                .as_ref()
                .into_iter()
                .flat_map(|value| env::split_paths(value)),
        );
    let path = env::join_paths(path).expect("test PATH");

    let output = Command::new(&installed[0].destination)
        .arg("system-chain")
        .current_dir(&workspace)
        .env("PINSET_HOME", &home)
        .env("PATH", path)
        .output()
        .expect("run system pnpm shim");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("system-node:system-chain"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("source=system"), "stdout: {stdout}");
}

#[test]
fn blocks_mutating_a_managed_flutter_sdk_before_invoking_it() {
    let root = tempdir().expect("temp directory");
    let project = root.path().join("project");
    let home = root.path().join("home");
    fs::create_dir_all(&project).expect("project");
    fs::write(
        project.join("pinset.toml"),
        "schema = 2\n[tools]\nflutter = \"3.47.0\"\n",
    )
    .expect("project config");
    create_fake_flutter(&home, "3.47.0");

    let output = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "flutter", "--cwd"])
        .arg(&project)
        .args(["--", "upgrade"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run shim");

    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "managed Flutter must not be invoked"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("refusing to run `flutter upgrade` against a Pinset-managed Flutter SDK"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dart = Command::new(env!("CARGO_BIN_EXE_pinset-shim"))
        .args(["--as", "dart", "--cwd"])
        .arg(&project)
        .args(["--", "--version"])
        .env("PINSET_HOME", &home)
        .output()
        .expect("run Dart shim");
    assert!(dart.status.success());
    let stdout = String::from_utf8_lossy(&dart.stdout);
    assert!(stdout.contains("dart:3.47.0:--version"), "stdout: {stdout}");
    assert!(stdout.contains("root="), "stdout: {stdout}");
}

fn create_fake_node(home: &Path, version: &str) -> PathBuf {
    let install_dir = home
        .join("installs")
        .join("node")
        .join(version)
        .join(current_target());
    let bin = if cfg!(windows) {
        install_dir
    } else {
        install_dir.join("bin")
    };
    fs::create_dir_all(&bin).expect("runtime bin");

    #[cfg(windows)]
    {
        let executable = bin.join("node.cmd");
        fs::write(
            &executable,
            "@echo off\r\nif \"%1\"==\"exit42\" exit /b 42\r\nif \"%1\"==\"spawn-node\" node child\r\nif \"%1\"==\"spawn-node\" exit /b %ERRORLEVEL%\r\nif \"%1\"==\"bridge\" pinset-test-bridge\r\nif \"%1\"==\"bridge\" exit /b %ERRORLEVEL%\r\nif \"%1\"==\"child\" echo node-child\r\nif \"%1\"==\"child\" echo chain=%PINSET_SHIM_CHAIN%\r\nif \"%1\"==\"child\" exit /b 0\r\necho %PINSET_SELECTED_VERSION%:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
        )
        .expect("fake node");
        executable
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = bin.join("node");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = \"exit42\" ]; then exit 42; fi\nif [ \"$1\" = \"spawn-node\" ]; then exec node child; fi\nif [ \"$1\" = \"bridge\" ]; then exec pinset-test-bridge; fi\nif [ \"$1\" = \"child\" ]; then printf 'node-child\\nchain=%s\\n' \"$PINSET_SHIM_CHAIN\"; exit 0; fi\nprintf '%s:%s\\nsource=%s\\n' \"$PINSET_SELECTED_VERSION\" \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
        )
        .expect("fake node");
        let mut permissions = fs::metadata(&executable)
            .expect("fake node metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake node permissions");
        executable
    }
}

fn create_fake_pnpm(home: &Path, version: &str) {
    let directory = home
        .join("installs/pnpm")
        .join(version)
        .join(pinset_core::current_target_for_tool("pnpm"));
    fs::create_dir_all(&directory).expect("pnpm command directory");

    #[cfg(windows)]
    fs::write(directory.join("pnpm.cmd"), "@echo off\r\nbun %*\r\n").expect("fake pnpm");

    #[cfg(not(windows))]
    write_executable(&directory.join("pnpm"), "#!/bin/sh\nexec bun \"$@\"\n");
}

fn create_fake_bun(home: &Path, version: &str) {
    let directory = home
        .join("installs/bun")
        .join(version)
        .join(pinset_core::current_target_for_tool("bun"))
        .join("bin");
    fs::create_dir_all(&directory).expect("Bun command directory");

    #[cfg(windows)]
    fs::write(
        directory.join("bun.cmd"),
        "@echo off\r\nif \"%1\"==\"chain3\" go final\r\nif \"%1\"==\"chain3\" exit /b %ERRORLEVEL%\r\necho bun:%*\r\necho chain=%PINSET_SHIM_CHAIN%\r\n",
    )
    .expect("fake Bun");

    #[cfg(not(windows))]
    write_executable(
        &directory.join("bun"),
        "#!/bin/sh\nif [ \"$1\" = \"chain3\" ]; then exec go final; fi\nprintf 'bun:%s\\nchain=%s\\n' \"$*\" \"$PINSET_SHIM_CHAIN\"\n",
    );
}

fn create_fake_go(home: &Path, version: &str) {
    let directory = home
        .join("installs/go")
        .join(version)
        .join(pinset_core::current_target_for_tool("go"))
        .join("bin");
    fs::create_dir_all(&directory).expect("Go command directory");

    #[cfg(windows)]
    fs::write(
        directory.join("go.cmd"),
        "@echo off\r\necho go:%*\r\necho chain=%PINSET_SHIM_CHAIN%\r\n",
    )
    .expect("fake Go");

    #[cfg(not(windows))]
    write_executable(
        &directory.join("go"),
        "#!/bin/sh\nprintf 'go:%s\\nchain=%s\\n' \"$*\" \"$PINSET_SHIM_CHAIN\"\n",
    );
}

#[cfg(not(windows))]
fn write_executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).expect("fake executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .expect("fake executable permissions");
}

fn create_fake_flutter(home: &Path, version: &str) {
    let bin = home
        .join("installs")
        .join("flutter")
        .join(version)
        .join(pinset_core::current_target_for_tool("flutter"))
        .join("bin");
    fs::create_dir_all(&bin).expect("Flutter bin");

    #[cfg(windows)]
    for command in ["flutter", "dart"] {
        fs::write(
            bin.join(format!("{command}.cmd")),
            format!("@echo off\r\necho {command}:%PINSET_SELECTED_VERSION%:%*\r\necho root=%FLUTTER_ROOT%\r\n"),
        )
        .expect("fake Flutter command");
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        for command in ["flutter", "dart"] {
            let executable = bin.join(command);
            fs::write(
                &executable,
                format!(
                    "#!/bin/sh\nprintf '{command}:%s:%s\\nroot=%s\\n' \"$PINSET_SELECTED_VERSION\" \"$*\" \"$FLUTTER_ROOT\"\n"
                ),
            )
            .expect("fake Flutter command");
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
                .expect("fake Flutter permissions");
        }
    }
}

fn create_fake_system_node(directory: &Path) -> PathBuf {
    fs::create_dir_all(directory).expect("system command directory");

    #[cfg(windows)]
    {
        let executable = directory.join("node.cmd");
        fs::write(
            &executable,
            "@echo off\r\necho system:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
        )
        .expect("fake system node");
        executable
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let executable = directory.join("node");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf 'system:%s\\nsource=%s\\n' \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
        )
        .expect("fake system node");
        let mut permissions = fs::metadata(&executable)
            .expect("fake system node metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake system node permissions");
        executable
    }
}

fn create_fake_system_pnpm_and_node(directory: &Path) {
    fs::create_dir_all(directory).expect("system command directory");

    #[cfg(windows)]
    {
        fs::write(directory.join("pnpm.cmd"), "@echo off\r\nnode %*\r\n")
            .expect("fake system pnpm");
        fs::write(
            directory.join("node.cmd"),
            "@echo off\r\necho system-node:%*\r\necho source=%PINSET_SELECTION_SOURCE%\r\n",
        )
        .expect("fake system node");
    }

    #[cfg(not(windows))]
    {
        write_executable(&directory.join("pnpm"), "#!/bin/sh\nexec node \"$@\"\n");
        write_executable(
            &directory.join("node"),
            "#!/bin/sh\nprintf 'system-node:%s\\nsource=%s\\n' \"$*\" \"$PINSET_SELECTION_SOURCE\"\n",
        );
    }
}

fn create_fake_system_bridge(directory: &Path) {
    fs::create_dir_all(directory).expect("system bridge directory");

    #[cfg(windows)]
    fs::write(
        directory.join("pinset-test-bridge.cmd"),
        "@echo off\r\nbun bridged\r\n",
    )
    .expect("fake system bridge");

    #[cfg(not(windows))]
    write_executable(
        &directory.join("pinset-test-bridge"),
        "#!/bin/sh\nexec bun bridged\n",
    );
}
