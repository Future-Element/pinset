//! Explicit, restartable preparation built on existing selection and installation operations.
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use pinset_core::{
    DiscoveryStatus, ReadinessState, find_optional_project_config, load_effective_project_config,
    load_optional_lockfile, lockfile_path, pinset_home,
};
use serde::{Deserialize, Serialize};

use crate::{
    i18n::Catalog,
    readiness::{self, ReportResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StepState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    state: StepState,
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupPlan {
    schema: u32,
    root: PathBuf,
    profile: Option<String>,
    requested_profile: Option<String>,
    environment: Option<pinset_core::EnvironmentDescriptor>,
    no_env: bool,
    fingerprint: String,
    selections: Vec<String>,
    steps: Vec<Step>,
    tasks: Vec<String>,
    blockers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetupRun {
    schema: u32,
    id: String,
    plan: SetupPlan,
}

fn step(id: &str) -> Step {
    Step {
        id: id.to_owned(),
        state: StepState::Pending,
        reason: None,
    }
}

pub fn plan(cwd: &Path, profile: Option<&str>, no_env: bool) -> ReportResult<SetupPlan> {
    let no_env = no_env || std::env::var_os("PINSET_ENV_DISABLE").is_some_and(|value| value == "1");
    let config_path = find_optional_project_config(cwd)?;
    let root = fs::canonicalize(config_path.as_deref().and_then(Path::parent).unwrap_or(cwd))?;
    let mut selections = Vec::new();
    let mut steps = Vec::new();
    let mut blockers = Vec::new();
    let mut tasks = Vec::new();
    let mut selected_profile = if no_env {
        None
    } else {
        profile.map(str::to_owned)
    };
    if let Some(path) = &config_path {
        let config = load_effective_project_config(path)?;
        if !no_env {
            selected_profile =
                pinset_core::environment_selection(&pinset_home()?, path, &config, profile)?
                    .profile;
        }
        if config.tools.is_empty() {
            blockers.push("No runtime is selected. Use pinset use <runtime>@<version>.".to_owned());
        }
        let lock = load_optional_lockfile(&lockfile_path(path))?;
        if let Some(lock) = &lock {
            pinset_core::validate_lock_matches_tools(lock, &config.tools, path)?;
            pinset_core::validate_lock_matches_tool_options(lock, &config.tool_options, path)?;
        } else {
            selections = config
                .tools
                .iter()
                .map(|(tool, version)| format!("{tool}@{version}"))
                .collect();
            steps.push(step("resolve"));
        }
        for provider in pinset_core::selected_provider_order(&config.tools)? {
            steps.push(step(&format!("install:{}", provider.tool)));
        }
        if config.tools.contains_key("python") {
            let supports_venv = lock
                .as_ref()
                .and_then(|lock| lock.tool("python"))
                .is_none_or(|locked| pinset_core::python_supports_stdlib_venv(&locked.version));
            if supports_venv {
                steps.push(step("venv:default"));
            }
            if let Some(python) = &config.python {
                for name in python.environments.keys() {
                    steps.push(step(&format!("venv:{name}")));
                }
            }
        }
        tasks = config.tasks.keys().cloned().collect();
    } else {
        let discovery = pinset_core::scan_project_sources(&root)?;
        for finding in discovery.findings {
            if matches!(
                finding.status,
                DiscoveryStatus::Conflict | DiscoveryStatus::Unsupported
            ) {
                blockers.push(format!(
                    "{}: {} ({})",
                    finding.tool,
                    finding
                        .reason
                        .as_deref()
                        .unwrap_or("selection requires review"),
                    finding.source
                ));
            }
            if finding.status == DiscoveryStatus::Ready
                && let Some(version) = finding.normalized
            {
                selections.push(format!("{}@{}", finding.tool, version));
            }
        }
        selections.sort();
        selections.dedup();
        if selections.is_empty() {
            blockers.push("No unambiguous runtime selections found. Use pinset init, then pinset use <runtime>@<version>.".to_owned());
        }
        steps.push(step("import"));
        steps.push(step("install-project"));
    }
    steps.push(step("environment"));
    steps.push(step("readiness"));
    let fingerprint = readiness::fingerprint(&root, selected_profile.as_deref(), no_env)?;
    let environment = if config_path.is_some() {
        Some(readiness::collect(&root, profile, no_env)?)
    } else {
        None
    };
    Ok(SetupPlan {
        schema: 1,
        root,
        profile: selected_profile,
        requested_profile: profile.map(str::to_owned),
        environment,
        no_env,
        fingerprint,
        selections,
        steps,
        tasks,
        blockers,
    })
}

pub struct SetupOptions<'a> {
    pub preview: bool,
    pub yes: bool,
    pub resume: Option<&'a str>,
    pub json: bool,
    pub offline: bool,
    pub profile: Option<&'a str>,
    pub no_env: bool,
    pub task: Option<&'a str>,
}

pub fn run(cwd: &Path, options: SetupOptions<'_>, catalog: Catalog) -> ReportResult<i32> {
    let options = SetupOptions {
        no_env: options.no_env
            || std::env::var_os("PINSET_ENV_DISABLE").is_some_and(|value| value == "1"),
        ..options
    };
    if options.preview {
        let plan = plan(cwd, options.profile, options.no_env)?;
        if options.json {
            crate::print_json_success("setup", &plan)?;
        } else {
            show(&plan, catalog);
        }
        return Ok(if plan.blockers.is_empty() { 0 } else { 1 });
    }
    // A separate setup lock serializes coordinators without nesting the install/config locks.
    let home = pinset_home()?;
    let root = fs::canonicalize(
        find_optional_project_config(cwd)?
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(cwd),
    )?;
    let _guard = pinset_core::acquire_setup_state_write_lock(&home, &root)?;
    let mut run = if let Some(id) = options.resume {
        let path = run_path(&home, id)?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024
        {
            return Err("setup run must be a regular file of at most 1 MiB".into());
        }
        let record: SetupRun = serde_json::from_slice(&fs::read(path)?)?;
        if record.schema != 1
            || record.plan.schema != 1
            || record.id != id
            || record.plan.root != root
            || record.plan.no_env != options.no_env
            || options
                .profile
                .is_some_and(|profile| record.plan.requested_profile.as_deref() != Some(profile))
        {
            return Err("setup run belongs to another context or unsupported schema".into());
        }
        record
    } else {
        SetupRun {
            schema: 1,
            id: uuid::Uuid::new_v4().to_string(),
            plan: plan(&root, options.profile, options.no_env)?,
        }
    };
    ensure_baseline(&run.plan)?;
    if options
        .task
        .is_some_and(|task| !run.plan.tasks.iter().any(|name| name == task))
    {
        return Err("setup task must be explicitly declared in pinset.toml".into());
    }
    if !run.plan.blockers.is_empty() {
        if options.json {
            crate::print_json_success("setup", &run.plan)?;
        } else {
            show(&run.plan, catalog);
        }
        return Ok(1);
    }
    if !options.yes {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(
                "setup requires --yes in non-interactive mode; preview with setup --plan --json"
                    .into(),
            );
        }
        show(&run.plan, catalog);
        eprint!("Prepare this environment? [y/N]: ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Ok(1);
        }
    }
    save(&home, &run)?;
    for index in 0..run.plan.steps.len() {
        ensure_baseline(&run.plan)?;
        // Re-run idempotent steps on resume so receipts/venvs are revalidated.
        run.plan.steps[index].state = StepState::Running;
        run.plan.steps[index].reason = None;
        save(&home, &run)?;
        let result = execute(
            &run.plan.steps[index].id,
            &run.plan,
            options.offline,
            options.json,
            catalog,
        )
        .and_then(|()| {
            if !matches!(run.plan.steps[index].id.as_str(), "import" | "resolve") {
                ensure_baseline(&run.plan)?;
            }
            Ok(())
        });
        match result {
            Ok(()) => {
                run.plan.steps[index].state = StepState::Succeeded;
                run.plan.fingerprint =
                    readiness::fingerprint(&root, run.plan.profile.as_deref(), run.plan.no_env)?;
                save(&home, &run)?;
            }
            Err(error) => {
                run.plan.steps[index].state = StepState::Failed;
                // Raw task/environment errors may contain secrets. Persist only a stable reason.
                run.plan.steps[index].reason =
                    Some(error.downcast_ref::<PreparationFailure>().map_or_else(
                        || crate::json_error(error.as_ref()).0.to_owned(),
                        |failure| failure.reason.clone(),
                    ));
                for item in run.plan.steps.iter_mut().skip(index + 1) {
                    item.state = StepState::Blocked;
                }
                save(&home, &run)?;
                if options.json {
                    crate::print_json_success(
                        "setup",
                        serde_json::json!({"run": run, "ready": false}),
                    )?;
                } else {
                    eprintln!(
                        "Setup step failed: {error}\nResume: pinset setup --resume {}",
                        run.id
                    );
                }
                return Ok(1);
            }
        }
    }
    let mut report = readiness::collect(
        &root,
        run.plan.requested_profile.as_deref(),
        run.plan.no_env,
    )?;
    readiness::verify_environment(&root, &mut report, run.plan.no_env);
    if report.environment_ready
        && let Some(task) = options.task
    {
        // Explicit task selection is separate from preparation. Its output stays out of JSON.
        let mut args = Vec::new();
        if run.plan.no_env {
            args.push("--no-env");
        } else if let Some(profile) = run.plan.requested_profile.as_deref() {
            args.extend(["-e", profile]);
        }
        args.extend(["run", task]);
        child(&root, &args, options.json)?;
    }
    if options.json {
        crate::print_json_success(
            "setup",
            serde_json::json!({"run": run, "report": report.portable()}),
        )?;
    } else {
        println!(
            "Setup {}: environment {}. Project tasks were not executed.",
            run.id,
            if report.environment_ready {
                "ready"
            } else {
                "needs attention"
            }
        );
        if !run.plan.tasks.is_empty() {
            println!("Available tasks: {}", run.plan.tasks.join(", "));
        }
    }
    Ok(if report.environment_ready { 0 } else { 1 })
}

fn execute(
    id: &str,
    plan: &SetupPlan,
    offline: bool,
    quiet: bool,
    catalog: Catalog,
) -> ReportResult<()> {
    let home = pinset_home()?;
    match id {
        "import" => {
            if find_optional_project_config(&plan.root)?.is_some() {
                return Ok(());
            }
            if offline {
                return Err("initial resolution requires metadata; prepare and import a lock before offline setup".into());
            }
            child(&plan.root, &["import", "--no-install"], quiet)
        }
        "resolve" => {
            let path = pinset_core::find_project_config(&plan.root)?;
            if load_optional_lockfile(&lockfile_path(&path))?.is_some() {
                return Ok(());
            }
            if offline {
                return Err(
                    "lock resolution requires metadata; use an existing lock for offline setup"
                        .into(),
                );
            }
            let mut args = vec!["use", "--no-install"];
            args.extend(plan.selections.iter().map(String::as_str));
            child(&plan.root, &args, quiet)
        }
        "install-project" => child(
            &plan.root,
            if offline {
                &["install", "--locked", "--offline"]
            } else {
                &["install", "--locked"]
            },
            quiet,
        ),
        "environment" => {
            let mut report =
                readiness::collect(&plan.root, plan.requested_profile.as_deref(), plan.no_env)?;
            readiness::verify_environment(&plan.root, &mut report, plan.no_env);
            if let Some(item) = report.checks.iter().find(|item| {
                item.id == "environment"
                    && matches!(item.state, ReadinessState::Fail | ReadinessState::Unknown)
            }) {
                return Err(Box::new(PreparationFailure {
                    reason: item.reason.clone(),
                }));
            }
            Ok(())
        }
        "readiness" => {
            let mut report =
                readiness::collect(&plan.root, plan.requested_profile.as_deref(), plan.no_env)?;
            readiness::verify_environment(&plan.root, &mut report, plan.no_env);
            if !report.environment_ready {
                return Err(
                    "project environment is not ready; run pinset check --report-version 2".into(),
                );
            }
            Ok(())
        }
        _ if id.starts_with("install:") => {
            let path = pinset_core::find_project_config(&plan.root)?;
            let lock = pinset_core::load_lockfile(&lockfile_path(&path))?;
            let config = load_effective_project_config(&path)?;
            pinset_core::validate_project_lock_policy(
                &config,
                &lock,
                std::time::SystemTime::now(),
            )?;
            crate::install_tool_from_lock_with_output(
                &home,
                &lock,
                &id[8..],
                false,
                offline,
                !quiet,
                catalog,
            )
        }
        _ if id.starts_with("venv:") => {
            let path = pinset_core::find_project_config(&plan.root)?;
            let config = load_effective_project_config(&path)?;
            let lock = pinset_core::load_lockfile(&lockfile_path(&path))?;
            let python = lock.tool("python").ok_or("Python is not locked")?;
            let name = &id[5..];
            if name == "default" && !pinset_core::python_supports_stdlib_venv(&python.version) {
                return Ok(());
            }
            crate::ensure_project_python_environment(
                &home,
                &path,
                name,
                crate::python_environment_relative_path(&config, name)?,
                &python.version,
                false,
                !quiet,
            )
        }
        _ => Err("unsupported setup step".into()),
    }
}

fn child(cwd: &Path, args: &[&str], quiet: bool) -> ReportResult<()> {
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe()?);
    command.current_dir(cwd).args(args).stdin(Stdio::null());
    if quiet {
        command.stdout(Stdio::null());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if !command.status()?.success() {
        return Err("preparation command failed; see stderr and run pinset check".into());
    }
    Ok(())
}

fn ensure_baseline(plan: &SetupPlan) -> ReportResult<()> {
    if !plan.no_env
        && let Some(path) = find_optional_project_config(&plan.root)?
    {
        let config = load_effective_project_config(&path)?;
        let selected = pinset_core::environment_selection(
            &pinset_home()?,
            &path,
            &config,
            plan.requested_profile.as_deref(),
        )?;
        if selected.profile != plan.profile {
            return Err("environment selection changed; review a new setup --plan".into());
        }
    }
    if readiness::fingerprint(&plan.root, plan.profile.as_deref(), plan.no_env)? != plan.fingerprint
    {
        return Err(
            "project inputs changed; review a new setup --plan instead of resuming stale work"
                .into(),
        );
    }
    Ok(())
}

fn run_path(home: &Path, id: &str) -> ReportResult<PathBuf> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err("invalid setup run ID".into());
    }
    Ok(home.join("state/setup").join(format!("{id}.json")))
}

fn save(home: &Path, run: &SetupRun) -> ReportResult<()> {
    let path = run_path(home, &run.id)?;
    let directory = path.parent().ok_or("missing setup directory")?;
    fs::create_dir_all(directory)?;
    if fs::symlink_metadata(directory)?.file_type().is_symlink()
        || fs::symlink_metadata(&path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err("setup state must be owned regular files".into());
    }
    let mut output = AtomicWriteFile::options().open(&path)?;
    output.write_all(&serde_json::to_vec_pretty(run)?)?;
    output.commit()?;
    Ok(())
}

fn show(plan: &SetupPlan, catalog: Catalog) {
    if let Some(environment) = &plan.environment {
        for runtime in &environment.runtimes {
            println!(
                "{}: {} (requested: {})",
                runtime.tool,
                runtime.locked_version.as_deref().unwrap_or("unlocked"),
                runtime.requested
            );
        }
    }
    if catalog.language() == crate::i18n::Language::SimplifiedChinese {
        println!("项目：{}", plan.root.display());
        for selection in &plan.selections {
            println!("选择：{selection}（解析版本需联网）");
        }
        for step in &plan.steps {
            println!("准备：{}", step.id);
        }
        for blocker in &plan.blockers {
            println!("需要处理：{blocker}");
        }
        println!("此预览不会解密变量或执行项目任务。");
        return;
    }
    println!("Project: {}", plan.root.display());
    for selection in &plan.selections {
        println!("Select: {selection} (resolution requires network)");
    }
    for step in &plan.steps {
        println!("Prepare: {}", step.id);
    }
    for blocker in &plan.blockers {
        println!("Needs decision: {blocker}");
    }
    println!("Encrypted values and project tasks are not read or executed by this preview.");
}

#[derive(Debug)]
struct PreparationFailure {
    reason: String,
}
impl std::fmt::Display for PreparationFailure {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            output,
            "environment precondition failed: {}; run pinset env check or pinset trust status",
            self.reason
        )
    }
}
impl std::error::Error for PreparationFailure {}
