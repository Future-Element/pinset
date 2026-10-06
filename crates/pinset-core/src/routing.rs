use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

pub const VENV_MARKER: &str = ".pinset-owner.toml";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VenvOwner {
    pub protocol: String,
    pub project_id: String,
    pub directory: WorkDirectoryIdentity,
    pub interpreter_identity: String,
    pub platform: String,
}
pub fn venv_python(root: &Path, platform: &str) -> PathBuf {
    if platform.starts_with("windows-") {
        root.join(".venv/Scripts/python.exe")
    } else {
        root.join(".venv/bin/python")
    }
}
pub fn validate_venv(
    context: &ProjectContext,
    config: &ProjectConfig,
    python: &LockedTool,
    platform: &str,
) -> Result<PathBuf> {
    let root = context.root.join(".venv");
    if fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(failure(
            "PINSET_VENV_EXTERNAL",
            "a linked .venv cannot be adopted",
        ));
    }
    let owner: VenvOwner =
        toml::from_str(&fs::read_to_string(root.join(VENV_MARKER)).map_err(|_| {
            failure(
                "PINSET_VENV_EXTERNAL",
                ".venv is missing or is not owned by Pinset 3",
            )
        })?)?;
    if owner.protocol != PROTOCOL
        || owner.project_id != config.project_id
        || owner.directory != context.directory
    {
        return Err(failure(
            "PINSET_VENV_EXTERNAL",
            ".venv belongs to a different project or directory",
        ));
    }
    if owner.interpreter_identity != python.installation_version(platform)
        || owner.platform != platform
    {
        return Err(failure(
            "PINSET_VENV_MISMATCH",
            "the interpreter changed; use install --recreate-venv",
        ));
    }
    let py = venv_python(&context.root, platform);
    let cfg = fs::read_to_string(root.join("pyvenv.cfg"))?;
    if !py.exists()
        || !cfg.lines().any(|l| {
            l.trim()
                .eq_ignore_ascii_case("include-system-site-packages = false")
        })
    {
        return Err(failure(
            "PINSET_VENV_DAMAGED",
            "venv interpreter or package isolation is invalid",
        ));
    }
    Ok(py)
}
pub fn command_tool(command: &str) -> Option<&'static str> {
    let command = command.strip_suffix(".exe").unwrap_or(command);
    if JAVA_COMMANDS.contains(&command) {
        return Some("java");
    }
    match command {
        "node" | "npm" | "npx" => Some("node"),
        "pnpm" | "pnpx" => Some("pnpm"),
        "bun" | "bunx" => Some("bun"),
        "go" | "gofmt" => Some("go"),
        "python" | "python3" | "pip" | "pip3" => Some("python"),
        "rustc" | "cargo" | "rustdoc" | "rustfmt" | "cargo-fmt" | "clippy-driver"
        | "cargo-clippy" | "rust-analyzer" | "miri" | "cargo-miri" => Some("rust"),
        "flutter" | "dart" => Some("flutter"),
        _ => None,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShimRegistry {
    protocol: String,
    digest: String,
    commands: BTreeMap<String, String>,
}
fn registered_command_tool(home: &Path, command: &str) -> Result<Option<&'static str>> {
    if let Some(tool) = command_tool(command) {
        return Ok(Some(tool));
    }
    let text = match fs::read_to_string(home.join("state/shims.json")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let registry: ShimRegistry = serde_json::from_str(&text)
        .map_err(|_| failure("PINSET_SHIM_STATE", "invalid managed command registry"))?;
    if registry.protocol != PROTOCOL
        || registry.digest.len() != 64
        || !registry.digest.bytes().all(|b| b.is_ascii_hexdigit())
        || registry.commands.iter().any(|(name, tool)| {
            validate_id(name).is_err()
                || matches!(name.as_str(), "pinset" | "pinset-shim")
                || !TOOLS.contains(&tool.as_str())
        })
    {
        return Err(failure(
            "PINSET_SHIM_STATE",
            "invalid managed command registry",
        ));
    }
    Ok(registry
        .commands
        .get(command.trim_end_matches(".exe"))
        .and_then(|name| TOOLS.iter().copied().find(|tool| *tool == name)))
}
pub fn public_commands() -> Vec<String> {
    let mut out = JAVA_COMMANDS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    out.extend(
        [
            "node",
            "npm",
            "npx",
            "pnpm",
            "pnpx",
            "bun",
            "bunx",
            "go",
            "gofmt",
            "python",
            "python3",
            "pip",
            "pip3",
            "rustc",
            "cargo",
            "rustdoc",
            "rustfmt",
            "cargo-fmt",
            "clippy-driver",
            "cargo-clippy",
            "rust-analyzer",
            "miri",
            "cargo-miri",
            "flutter",
            "dart",
        ]
        .into_iter()
        .map(str::to_string),
    );
    out
}
pub fn sdk_root(install: &Path, tool: &str, target: &str) -> PathBuf {
    if tool == "java" && target.starts_with("macos-") {
        install.join("Contents/Home")
    } else {
        install.to_path_buf()
    }
}
pub fn locked_target(tool: &LockedTool) -> String {
    let target = current_target_for_tool(&tool.name);
    if tool.name == "bun" && target.ends_with("-avx2") && tool.artifact(&target).is_none() {
        let baseline = target.replace("-avx2", "-baseline");
        if tool.artifact(&baseline).is_some() {
            return baseline;
        }
    }
    target
}
pub fn executable_name(name: &str, target: &str) -> String {
    if target.starts_with("windows-") {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}
pub fn command_directory(install: &Path, tool: &str, target: &str) -> PathBuf {
    if target.starts_with("windows-") && matches!(tool, "node" | "python") {
        install.to_path_buf()
    } else {
        sdk_root(install, tool, target).join("bin")
    }
}
pub fn interpreter_path(install: &Path, target: &str) -> PathBuf {
    if target.starts_with("windows-") {
        install.join("python.exe")
    } else {
        install.join("bin/python3")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandPlan {
    pub protocol: String,
    pub project: Option<PathBuf>,
    pub command: String,
    pub executable: PathBuf,
    pub prefix: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub remove_environment: Vec<String>,
    pub tool: Option<String>,
    pub version: Option<String>,
    pub install_identity: Option<String>,
    pub sdk: Option<PathBuf>,
    pub source: String,
    pub provider: Option<String>,
    pub build: Option<String>,
    pub artifact: Option<LockedArtifact>,
}
pub fn plan_command(
    cwd: &Path,
    home: &Path,
    context: &ProjectContext,
    command: &str,
) -> Result<CommandPlan> {
    let (config, lock) = context.load_locked()?;
    let mut variables = BTreeMap::new();
    let mut paths = Vec::new();
    // Shim directory prevents child processes falling through to another managed tool.
    paths.push(home.join("bin"));
    let mut selected = BTreeMap::new();
    for tool in &lock.tools {
        let target = locked_target(tool);
        let receipt = read_receipt(home, tool, &target)?;
        let install = install_directory(home, tool, &target);
        let sdk = sdk_root(&install, &tool.name, &target);
        paths.push(command_directory(&install, &tool.name, &target));
        match tool.name.as_str() {
            "java" => {
                variables.insert("JAVA_HOME".into(), sdk.display().to_string());
            }
            "go" => {
                variables.insert("GOROOT".into(), sdk.display().to_string());
                variables.insert("GOTOOLCHAIN".into(), "local".into());
            }
            "rust" => {
                variables.insert("RUSTUP_TOOLCHAIN".into(), sdk.display().to_string());
            }
            "flutter" => {
                variables.insert("FLUTTER_ROOT".into(), sdk.display().to_string());
            }
            "pnpm" => {
                variables.insert(
                    "npm_config_manage_package_manager_versions".into(),
                    "false".into(),
                );
                variables.insert(
                    "npm_config_package_manager_strict_version".into(),
                    "true".into(),
                );
            }
            "python" => {
                variables.insert("PYTHONNOUSERSITE".into(), "1".into());
                if !context.global {
                    let py = validate_venv(context, &config, tool, &target)?;
                    paths.push(py.parent().unwrap().to_path_buf());
                    variables.insert(
                        "VIRTUAL_ENV".into(),
                        context.root.join(".venv").display().to_string(),
                    );
                }
            }
            _ => {}
        }
        selected.insert(tool.name.as_str(), (tool, target, install, sdk, receipt));
    }
    if let Some(path) = env::var_os("PATH") {
        paths.extend(
            env::split_paths(&path)
                .filter(|p| !paths.contains(p))
                .collect::<Vec<_>>(),
        );
    }
    let path = env::join_paths(paths).map_err(|e| failure("PINSET_PATH_INVALID", e.to_string()))?;
    variables.insert("PATH".into(), path.to_string_lossy().into_owned());
    let base = Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);
    let managed = if Path::new(command).components().count() == 1 {
        registered_command_tool(home, base)?.or_else(|| {
            selected
                .get("java")
                .filter(|(_, _, _, _, r)| r.commands.contains_key(base.trim_end_matches(".exe")))
                .map(|_| "java")
        })
    } else {
        None
    };
    let mut plan = CommandPlan {
        protocol: PROTOCOL.into(),
        project: (!context.global).then(|| context.root.clone()),
        command: command.into(),
        executable: PathBuf::new(),
        prefix: vec![],
        environment: variables,
        remove_environment: vec![
            "PINSET_IDENTITY".into(),
            "PYTHONHOME".into(),
            "PYTHONPATH".into(),
            "RUSTUP_HOME".into(),
        ],
        provider: None,
        build: None,
        artifact: None,
        tool: None,
        version: None,
        install_identity: None,
        sdk: None,
        source: if context.global { "global" } else { "project" }.into(),
    };
    if let Some(name) = managed {
        let (tool, target, install, sdk, receipt) = selected.get(name).ok_or_else(|| {
            failure(
                "PINSET_TOOL_NOT_SELECTED",
                format!("{name} is not selected in this {}", plan.source),
            )
        })?;
        plan.provider = Some(tool.provider.clone());
        plan.build = tool.version.split_once('+').map(|(_, build)| build.into());
        plan.artifact = tool.artifact(target).cloned();
        plan.tool = Some(name.into());
        plan.version = Some(tool.version.clone());
        plan.install_identity = Some(tool.installation_version(target));
        plan.sdk = Some(sdk.clone());
        let cmd = base.trim_end_matches(".exe");
        match (name, cmd) {
            ("python", _) => {
                plan.executable = if context.global {
                    interpreter_path(install, target)
                } else {
                    validate_venv(context, &config, tool, target)?
                };
                if cmd.starts_with("pip") {
                    plan.prefix = vec!["-m".into(), "pip".into()];
                }
            }
            ("node", "npm" | "npx") => {
                plan.executable =
                    command_directory(install, name, target).join(executable_name("node", target));
                let script = if cmd == "npm" {
                    "npm-cli.js"
                } else {
                    "npx-cli.js"
                };
                let npm = if target.starts_with("windows-") {
                    install.join("node_modules/npm/bin")
                } else {
                    install.join("lib/node_modules/npm/bin")
                };
                plan.prefix.push(npm.join(script).display().to_string());
            }
            ("pnpm", _) => {
                let (_, nt, ni, _, _) = selected
                    .get("node")
                    .ok_or_else(|| failure("PINSET_DEPENDENCY_REQUIRED", "pnpm requires Node"))?;
                plan.executable =
                    command_directory(ni, "node", nt).join(executable_name("node", nt));
                plan.prefix
                    .push(install.join("bin/pnpm.cjs").display().to_string());
                // pnpm ignores an empty environment setting, but an explicit empty CLI value
                // overrides project use-node-version without choosing another runtime.
                plan.prefix.push("--use-node-version=".into());
                if cmd == "pnpx" {
                    plan.prefix.push("dlx".into());
                }
            }
            ("flutter", "dart") => {
                plan.executable = install
                    .join("bin/cache/dart-sdk/bin")
                    .join(executable_name("dart", target));
            }
            ("java", _) => {
                let entry = receipt.commands.get(cmd).ok_or_else(|| {
                    failure(
                        "PINSET_COMMAND_UNAVAILABLE",
                        format!(
                            "Temurin {} does not provide {cmd} on {target}",
                            tool.version
                        ),
                    )
                })?;
                plan.executable = install.join(entry);
            }
            ("bun", "bunx") => {
                plan.executable =
                    command_directory(install, name, target).join(executable_name("bun", target));
                plan.prefix.push("x".into());
            }
            _ => {
                let filename = if name == "flutter" && target.starts_with("windows-") {
                    format!("{cmd}.bat")
                } else {
                    executable_name(cmd, target)
                };
                plan.executable = command_directory(install, name, target).join(filename);
            }
        }
        if !plan.executable.is_file()
            || plan
                .prefix
                .first()
                .is_some_and(|p| Path::new(p).is_absolute() && !Path::new(p).is_file())
        {
            return Err(failure(
                "PINSET_COMMAND_UNAVAILABLE",
                format!("the selected {name} does not provide {cmd}"),
            ));
        }
    } else {
        let p = Path::new(command);
        plan.executable = if p.is_absolute() {
            p.to_path_buf()
        } else if p.components().count() > 1 {
            cwd.join(p)
        } else {
            env::split_paths(&std::ffi::OsString::from(&plan.environment["PATH"]))
                .flat_map(|p| {
                    if cfg!(windows) {
                        vec![
                            p.join(command),
                            p.join(format!("{command}.exe")),
                            p.join(format!("{command}.cmd")),
                            p.join(format!("{command}.bat")),
                        ]
                    } else {
                        vec![p.join(command)]
                    }
                })
                .find(|p| p.is_file())
                .ok_or_else(|| {
                    failure(
                        "PINSET_COMMAND_NOT_FOUND",
                        format!("{command} was not found"),
                    )
                })?
        };
        if !plan.executable.is_file() {
            return Err(failure(
                "PINSET_COMMAND_NOT_FOUND",
                format!("{command} was not found"),
            ));
        }
        plan.source = "external-command".into();
    }
    Ok(plan)
}
