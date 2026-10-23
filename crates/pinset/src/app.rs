use crate::args::*;
use clap::{CommandFactory, Parser};
use pinset_engine::{Result, Services, service_error};
use serde_json::Value;
use std::io::{self, IsTerminal, Read, Write};

pub fn start() -> i32 {
    let raw = std::env::args().collect::<Vec<_>>();
    if raw.get(1).map(String::as_str) == Some("__process-worker") {
        return pinset_engine::process_worker();
    }
    if raw.get(1).map(String::as_str) == Some("__env-resolve") {
        return match broker(&raw) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let display = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            if !display && requested_json(&raw) {
                let mut value = serde_json::json!({"protocol":pinset_core::PROTOCOL,"error":{"code":"PINSET_ARGUMENT_INVALID","message":error.to_string()}});
                if raw.iter().any(|arg| arg == "check") {
                    value["schema_version"] = 1.into();
                    value["passed"] = false.into();
                }
                println!("{}", json_document(&value));
            } else {
                let _ = error.print();
            }
            return if display { 0 } else { 2 };
        }
    };
    let chinese = cli.lang == "zh-CN"
        || (cli.lang == "auto"
            && ["LANG", "LC_ALL"]
                .iter()
                .any(|key| std::env::var(key).is_ok_and(|v| v.starts_with("zh"))));
    match dispatch(&cli, chinese) {
        Ok(Output::Native(code)) => code,
        Ok(Output::Text(s)) => {
            print!("{s}");
            0
        }
        Ok(Output::Report(value)) => {
            if cli.json_output() {
                println!("{}", json_document(&value));
            } else {
                print!("{}", crate::presentation::report(&value, chinese));
            }
            if matches!(cli.command, Commands::Check { .. }) && value["passed"] != true {
                1
            } else {
                0
            }
        }
        Err(error) => {
            let code = error.code();
            if cli.json_output() {
                let mut value = serde_json::json!({"protocol":pinset_core::PROTOCOL,"error":{"code":code,"message":error.to_string()}});
                if matches!(cli.command, Commands::Check { .. }) {
                    value["schema_version"] = 1.into();
                    value["passed"] = false.into();
                }
                println!("{}", json_document(&value));
            } else {
                if chinese {
                    eprint!("操作失败：");
                }
                eprintln!("{error}");
            }
            1
        }
    }
}
enum Output {
    Report(Value),
    Text(String),
    Native(i32),
}
fn json_document(value: &Value) -> String {
    let mut value = value.clone();
    value["schema_version"] = 1.into();
    serde_json::to_string(&value).expect("JSON reports are serializable")
}
fn report(value: Value) -> Result<Output> {
    Ok(Output::Report(value))
}
fn dispatch(cli: &Cli, chinese: bool) -> Result<Output> {
    let progress = crate::progress::ProgressDisplay::new(
        !cli.json_output() && io::stderr().is_terminal(),
        chinese,
    );
    let service = Services::new(&cli.cwd)?.with_progress_reporter(progress.reporter());
    match &cli.command {
        Commands::Init => report(service.init()?),
        Commands::Import { plan, yes } => {
            let prepared = service.plan_import()?;
            let preview = prepared.report();
            // JSON stays one document on stdout. Without --yes it is a read-only plan.
            if *plan || (cli.json && !*yes) {
                return report(preview);
            }
            if cli.json {
                eprintln!("{}", json_document(&preview));
            } else {
                print!("{}", crate::presentation::report(&preview, chinese));
                io::stdout().flush()?;
            }
            if prepared.needs_write() && !*yes {
                eprint!(
                    "{}",
                    if chinese {
                        "将以上结果写入配置和锁文件？[y/N] "
                    } else {
                        "Write this plan to config and lock files? [y/N] "
                    }
                );
                io::stderr().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                if !matches!(
                    answer.trim().to_ascii_lowercase().as_str(),
                    "y" | "yes" | "是"
                ) {
                    return report(serde_json::json!({"protocol":pinset_core::PROTOCOL,
                        "operation":"import","plan":false,"applied":false,"cancelled":true}));
                }
            }
            report(service.apply_import(prepared)?)
        }
        Commands::Use {
            tools,
            global,
            no_install,
            plan,
        } => report(service.use_tools(tools, *global, *no_install, *plan)?),
        Commands::Remove {
            tools,
            global,
            plan,
        } => report(service.remove(tools, *global, *plan)?),
        Commands::Install {
            tools,
            global,
            offline,
            repair,
            recreate_venv,
            plan,
        } => report(service.install(tools, *global, *offline, *repair, *recreate_venv, *plan)?),
        Commands::List { tool, remote } => report(if *remote {
            service.remote(tool.as_deref().unwrap())?
        } else {
            service.list(tool.as_deref())?
        }),
        Commands::Which {
            command,
            global,
            explain,
        } => {
            let value = service.which(command.as_deref(), *global)?;
            if !cli.json && !*explain && command.is_some() {
                Ok(Output::Text(format!(
                    "{}\n",
                    value["executable"].as_str().unwrap_or("")
                )))
            } else {
                report(value)
            }
        }
        Commands::Check {
            format: _,
            global,
            deep,
            probe,
            target,
        } => report(service.check(
            *global,
            *deep,
            *probe,
            target.as_ref().map(|t| match t {
                BuildTarget::Android => "android",
                BuildTarget::Ios => "ios",
                BuildTarget::Windows => "windows",
                BuildTarget::Macos => "macos",
                BuildTarget::Linux => "linux",
                BuildTarget::Web => "web",
            }),
        )?),
        Commands::Exec {
            environment,
            command,
        } => {
            native_output(cli)?;
            Ok(Output::Native(service.exec(
                command,
                environment.profile.as_deref(),
                environment.no_env,
            )?))
        }
        Commands::Env { command } => match command {
            Environment::Init { profile } => report(service.profile_init(profile)?),
            Environment::Use {
                profile,
                reset: _,
                project,
            } => report(service.profile_use(profile.as_deref(), *project)?),
            Environment::Remove { profile, plan } => {
                report(service.profile_remove(profile, *plan)?)
            }
            Environment::List { profile } => report(service.profile_list(profile.as_deref())?),
            Environment::Set {
                name,
                profile,
                stdin,
            } => {
                let value = if *stdin {
                    let mut bytes = String::new();
                    io::stdin()
                        .take(1024 * 1024 + 1)
                        .read_to_string(&mut bytes)?;
                    if bytes.len() > 1024 * 1024 {
                        return Err(service_error(
                            "PINSET_ENV_INPUT_LIMIT",
                            "environment value exceeds 1 MiB",
                        ));
                    }
                    bytes.trim_end_matches(['\r', '\n']).to_string()
                } else {
                    rpassword::prompt_password(format!("{name}: "))?
                };
                report(service.profile_set(profile, name, value)?)
            }
            Environment::Unset { name, profile } => report(service.profile_unset(profile, name)?),
            Environment::Trust { command } => report(service.trust(match command {
                Trust::Add => "add",
                Trust::Status => "status",
                Trust::Revoke => "revoke",
            })?),
            Environment::Access { command } => match command {
                Access::Request { new, ci } => {
                    if *ci {
                        if cli.json || !io::stdout().is_terminal() || !io::stdin().is_terminal() {
                            return Err(service_error(
                                "PINSET_CI_IDENTITY_TTY",
                                "--ci requires an interactive TTY and cannot use --json",
                            ));
                        }
                        let (path, secret) = service.ci_access_request()?;
                        println!("Public request: {}", path.display());
                        println!(
                            "Store this identity in the platform Secret; it is displayed once:\n{}",
                            secret.as_str()
                        );
                        Ok(Output::Native(0))
                    } else {
                        report(service.access_request(*new)?)
                    }
                }
                Access::Grant {
                    request_file,
                    profile,
                } => report(service.access_grant(request_file, profile)?),
                Access::Revoke {
                    request_id,
                    profile,
                } => report(service.access_revoke(request_id, profile)?),
                Access::List { profile } => report(service.access_list(profile)?),
            },
        },
        Commands::Clean { command } => report(match command {
            Clean::Cache { plan } => service.clean("cache", &[], *plan)?,
            Clean::Installs { tools, plan } => service.clean("installs", tools, *plan)?,
        }),
        Commands::SelfCommand { command } => match command {
            SelfCommand::Info => report(service.self_info()),
            SelfCommand::Repair { plan } => report(service.self_repair(*plan)?),
            SelfCommand::Shell { shell } => output_text(cli, service.shell(shell_name(*shell))),
            SelfCommand::Completions { shell } => {
                let mut command = Cli::command();
                let mut bytes = vec![];
                clap_complete::generate(
                    match shell {
                        Shell::Bash => clap_complete::Shell::Bash,
                        Shell::Zsh => clap_complete::Shell::Zsh,
                        Shell::Fish => clap_complete::Shell::Fish,
                        Shell::Powershell => clap_complete::Shell::PowerShell,
                    },
                    &mut command,
                    "pinset",
                    &mut bytes,
                );
                output_text(cli, String::from_utf8_lossy(&bytes).into_owned())
            }
            SelfCommand::Update { version, plan } => {
                report(service.self_update(version.as_deref(), *plan)?)
            }
        },
    }
}
fn requested_json(raw: &[String]) -> bool {
    // Do not interpret arguments forwarded to a child process.
    let args = raw
        .iter()
        .take_while(|arg| arg.as_str() != "--")
        .collect::<Vec<_>>();
    args.iter().any(|arg| arg.as_str() == "--json")
        || (args.iter().any(|arg| arg.as_str() == "check")
            && (args.iter().any(|arg| arg.as_str() == "--format=json")
                || args
                    .windows(2)
                    .any(|pair| pair[0] == "--format" && pair[1] == "json")))
}
fn output_text(cli: &Cli, text: String) -> Result<Output> {
    if cli.json {
        report(serde_json::json!({"protocol":pinset_core::PROTOCOL,"text":text}))
    } else {
        Ok(Output::Text(text))
    }
}
fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Zsh => "zsh",
        Shell::Fish => "fish",
        Shell::Powershell => "powershell",
    }
}
fn native_output(cli: &Cli) -> Result<()> {
    if cli.json {
        Err(service_error(
            "PINSET_NATIVE_OUTPUT",
            "exec preserves native output and reject --json",
        ))
    } else {
        Ok(())
    }
}
fn broker(args: &[String]) -> Result<()> {
    let get = |key: &str| {
        args.iter()
            .position(|s| s == key)
            .and_then(|i| args.get(i + 1))
    };
    if get("--protocol").map(String::as_str) != Some(pinset_core::PROTOCOL)
        || get("--version").map(String::as_str) != Some(pinset_core::pinset_version())
    {
        return Err(service_error(
            "PINSET_BROKER_VERSION",
            "CLI and shim must use the same v3 protocol and version",
        ));
    }
    let cwd = get("-C")
        .ok_or_else(|| service_error("PINSET_BROKER_CWD", "broker requires an explicit cwd"))?;
    let service = Services::new(std::path::Path::new(cwd))?;
    let c = service.context(false)?;
    let values = service.resolve_profile(&c, None, false)?;
    let mut bytes = pinset_core::encode_environment(&values)
        .map_err(|e| service_error("PINSET_BROKER_PROTOCOL", e))?;
    use zeroize::Zeroize;
    io::stdout().write_all(&bytes)?;
    bytes.zeroize();
    Ok(())
}
