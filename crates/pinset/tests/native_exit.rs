//! Windows process statuses must survive both the public CLI and probe worker.
//! Linux cannot observe statuses outside its eight-bit process exit range.
#![cfg(windows)]

use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn native_exec_and_private_worker_preserve_windows_statuses() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let cli = env!("CARGO_BIN_EXE_pinset");
    let shell = std::env::var_os("COMSPEC").expect("Windows must provide COMSPEC");
    let mut init = Command::new(cli);
    init.args(["-C", project.to_str().unwrap(), "init"])
        .env("PINSET_HOME", &home)
        .env_remove("PINSET_IDENTITY");
    assert!(init.status().unwrap().success());
    for expected in [
        0,
        1,
        256,
        3010,
        -1,
        0xc0000005_u32 as i32,
        0xc0000100_u32 as i32,
    ] {
        let status = expected.to_string();
        let public = Command::new(cli)
            .args(["-C", project.to_str().unwrap(), "exec", "--no-env", "--"])
            .arg(&shell)
            .args(["/D", "/C", "exit", "/b", &status])
            .env("PINSET_HOME", &home)
            .env_remove("PINSET_IDENTITY")
            .status()
            .unwrap();
        assert_eq!(public.code(), Some(expected), "public exec: {status}");
        let mut worker = Command::new(cli)
            .arg("__process-worker")
            .arg(&shell)
            .args(["/D", "/C", "exit", "/b", &status])
            .stdin(Stdio::piped())
            .env_remove("PINSET_IDENTITY")
            .spawn()
            .unwrap();
        worker.stdin.take().unwrap().write_all(b"G").unwrap();
        assert_eq!(
            worker.wait().unwrap().code(),
            Some(expected),
            "private worker: {status}"
        );
    }
}
