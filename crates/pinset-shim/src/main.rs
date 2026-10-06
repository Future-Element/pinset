use pinset_core::*;
use std::{
    env,
    io::Read,
    path::Path,
    process::{Command, Stdio},
};
use zeroize::Zeroize;
fn run() -> Result<i32> {
    let argv = env::args_os().collect::<Vec<_>>();
    let current = env::current_exe()?;
    let name = Path::new(&argv[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| failure("PINSET_COMMAND_INVALID", "invalid shim name"))?;
    if name == "pinset-shim" {
        return Err(failure(
            "PINSET_COMMAND_INVALID",
            "invoke a managed command entry",
        ));
    }
    let cwd = env::current_dir()?;
    let home = pinset_home()?;
    let c = selected_context(&cwd, false)?;
    let plan = plan_command(&cwd, &home, &c, name)?;
    if plan.executable.canonicalize()? == current.canonicalize()? {
        return Err(failure("PINSET_ROUTE_RECURSION", "shim resolves to itself"));
    }
    let cli = current
        .parent()
        .ok_or_else(|| failure("PINSET_BROKER_MISSING", "missing adjacent CLI"))?
        .join(if cfg!(windows) {
            "pinset.exe"
        } else {
            "pinset"
        });
    let mut child = Command::new(cli)
        .arg("__env-resolve")
        .arg("--protocol")
        .arg(PROTOCOL)
        .arg("--version")
        .arg(pinset_version())
        .arg("-C")
        .arg(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_| failure("PINSET_BROKER_MISSING", "matching adjacent CLI is required"))?;
    let mut payload = vec![];
    child
        .stdout
        .take()
        .unwrap()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut payload)?;
    if !child.wait()?.success() {
        payload.zeroize();
        return Err(failure("PINSET_ENV_DENIED", "broker refused execution"));
    }
    let values = decode_environment(&payload).map_err(|e| failure("PINSET_BROKER_PROTOCOL", e))?;
    payload.zeroize();
    let mut cmd = Command::new(&plan.executable);
    cmd.args(&plan.prefix)
        .args(&argv[1..])
        .envs(&plan.environment);
    for key in &plan.remove_environment {
        cmd.env_remove(key);
    }
    for (key, mut value) in values {
        cmd.env(key, &value);
        value.zeroize();
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(cmd.exec().into())
    }
    #[cfg(windows)]
    {
        Ok(cmd.status()?.code().unwrap_or(1))
    }
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
