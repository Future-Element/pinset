//! Candidate tasks start behind a private pipe handshake. On Windows the
//! worker joins a kill-on-close Job before it can create the actual task.
use std::{
    io::{self, Read, Write},
    process::{Child, Command, Stdio},
    sync::{
        Once,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

static CANCELED: AtomicBool = AtomicBool::new(false);
static SIGNALS: Once = Once::new();
pub type ProcessResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn worker() -> i32 {
    let mut release = [0];
    if io::stdin().read_exact(&mut release).is_err() || release != *b"G" {
        return 125;
    }
    let mut arguments = std::env::args_os().skip(2);
    let Some(program) = arguments.next() else {
        return 125;
    };
    match Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(_) => 127,
    }
}

pub struct Outcome {
    pub code: i32,
    pub stdout: Vec<u8>,
}

pub fn run(
    command: Command,
    timeout: Duration,
    output_limit: Option<u64>,
) -> ProcessResult<Outcome> {
    install_signals();
    if CANCELED.load(Ordering::SeqCst) {
        return Ok(Outcome {
            code: 130,
            stdout: Vec::new(),
        });
    }
    let mut launcher = Command::new(std::env::current_exe()?);
    launcher
        .arg("__candidate-worker")
        .arg(command.get_program())
        .args(command.get_args())
        .env_clear();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            launcher.env(key, value);
        }
    }
    if let Some(cwd) = command.get_current_dir() {
        launcher.current_dir(cwd);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        launcher.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        launcher.creation_flags(0x08000000);
    }
    launcher.stdin(Stdio::piped());
    launcher.stderr(if output_limit.is_some() {
        Stdio::null()
    } else {
        Stdio::inherit()
    });
    if output_limit.is_some() {
        launcher.stdout(Stdio::piped());
    } else {
        launcher.stdout(Stdio::inherit());
    }
    let child = launcher.spawn()?;
    let mut tree = Tree::new(child)?;
    // Assignment failure drops the worker before any project command runs.
    tree.child
        .stdin
        .take()
        .ok_or("missing worker control pipe")?
        .write_all(b"G")?;
    let receiver = if let Some(limit) = output_limit {
        let output = tree.child.stdout.take().ok_or("missing worker output")?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = output
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        });
        Some(receiver)
    } else {
        None
    };
    let start = Instant::now();
    let mut output = None;
    loop {
        if CANCELED.load(Ordering::SeqCst) {
            return Ok(Outcome {
                code: 130,
                stdout: Vec::new(),
            });
        }
        if start.elapsed() >= timeout {
            return Ok(Outcome {
                code: 124,
                stdout: Vec::new(),
            });
        }
        if let Some(receiver) = &receiver
            && let Ok(bytes) = receiver.try_recv()
        {
            let bytes = bytes?;
            if bytes.len() as u64 > output_limit.unwrap_or(0) {
                return Err("candidate input listing exceeded its output limit".into());
            }
            output = Some(bytes);
        }
        if let Some(status) = tree.child.try_wait()? {
            tree.terminate();
            if receiver.is_none() || output.is_some() {
                return Ok(Outcome {
                    code: status.code().unwrap_or(1),
                    stdout: output.unwrap_or_default(),
                });
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct Tree {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
impl Tree {
    fn new(mut child: Child) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            // SAFETY: null names create private, non-inherited jobs; the process
            // handle belongs to Child and all buffers have their declared size.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    let error = io::Error::last_os_error();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const _,
                    std::mem::size_of_val(&info) as u32,
                ) == 0
                    || AssignProcessToJobObject(job, child.as_raw_handle()) == 0
                {
                    let error = io::Error::last_os_error();
                    CloseHandle(job);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
                Ok(Self { child, job })
            }
        }
        #[cfg(not(windows))]
        {
            let _ = &mut child;
            Ok(Self { child })
        }
    }
    fn terminate(&mut self) {
        #[cfg(unix)]
        // SAFETY: the worker was spawned into a new group whose ID is its PID.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        // SAFETY: this job is owned by this Tree and never shared/inherited.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        self.terminate();
        #[cfg(windows)]
        // SAFETY: the job handle was created in Tree::new and is closed once.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

fn install_signals() {
    SIGNALS.call_once(|| {
        #[cfg(unix)]
        {
            extern "C" fn canceled(_: i32) {
                CANCELED.store(true, Ordering::SeqCst);
            }
            // SAFETY: the handler only stores to a lock-free atomic and remains
            // installed for this short-lived CLI process.
            unsafe {
                libc::signal(libc::SIGINT, canceled as *const () as libc::sighandler_t);
                libc::signal(libc::SIGTERM, canceled as *const () as libc::sighandler_t);
            }
        }
        #[cfg(windows)]
        {
            unsafe extern "system" fn canceled(kind: u32) -> i32 {
                if kind <= 2 {
                    CANCELED.store(true, Ordering::SeqCst);
                    1
                } else {
                    0
                }
            }
            // SAFETY: the callback has static lifetime and only changes an atomic.
            unsafe {
                windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(canceled), 1);
            }
        }
    });
}

pub fn system_environment(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "TERM",
        "COMPUTERNAME",
        "HOSTNAME",
        "WSL_DISTRO_NAME",
        "SystemDrive",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}
