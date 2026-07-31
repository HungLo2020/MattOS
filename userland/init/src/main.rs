#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{self, BufRead, Write};
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
fn mount_fs(source: Option<&str>, target: &str, fstype: &str, flags: libc::c_ulong) -> io::Result<()> {
    let source = source.map(|s| CString::new(s).expect("valid source CString"));
    let target = CString::new(target).expect("valid target CString");
    let fstype = CString::new(fstype).expect("valid fstype CString");

    let rc = unsafe {
        libc::mount(
            source.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            target.as_ptr(),
            fstype.as_ptr(),
            flags,
            std::ptr::null(),
        )
    };

    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn try_mounts() {
    let _ = fs::create_dir_all("/proc");
    let _ = fs::create_dir_all("/sys");
    let _ = fs::create_dir_all("/dev");
    let _ = fs::create_dir_all("/tmp");

    if let Err(err) = mount_fs(Some("proc"), "/proc", "proc", 0) {
        eprintln!("mattos-init: mount /proc failed: {err}");
    }
    if let Err(err) = mount_fs(Some("sysfs"), "/sys", "sysfs", 0) {
        eprintln!("mattos-init: mount /sys failed: {err}");
    }
    if let Err(err) = mount_fs(Some("devtmpfs"), "/dev", "devtmpfs", 0) {
        eprintln!("mattos-init: mount /dev failed: {err}");
    }
    if let Err(err) = mount_fs(
        Some("tmpfs"),
        "/tmp",
        "tmpfs",
        (libc::MS_NOSUID | libc::MS_NODEV) as libc::c_ulong,
    ) {
        eprintln!("mattos-init: mount /tmp failed: {err}");
    }
}

#[cfg(unix)]
fn reap_zombies_nonblocking() {
    loop {
        let mut status: libc::c_int = 0;
        let pid = unsafe { libc::waitpid(-1, &mut status as *mut _, libc::WNOHANG) };
        if pid <= 0 {
            break;
        }
    }
}

#[cfg(unix)]
fn spawn_brush() -> io::Result<Child> {
    Command::new("/bin/brush")
        .arg("-i")
    .arg("--no-config")
    .arg("--noprofile")
    .arg("--norc")
    .arg("--noediting")
    .arg("--input-backend")
    .arg("basic")
        .env("PS1", "MattOS # ")
        .env("PATH", "/bin:/usr/bin:/sbin:/usr/sbin")
    .env("HOME", "/root")
    .env("TERM", "linux")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

#[cfg(unix)]
fn run_command_line(line: &str) -> io::Result<()> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    let cmd = parts[0];
    let args = &parts[1..];

    if cmd == "echo" {
        println!("{}", args.join(" "));
        return Ok(());
    }

    let candidate = format!("/bin/{cmd}");
    let exec = if fs::metadata(&candidate).is_ok() {
        candidate
    } else if cmd.starts_with('/') {
        cmd.to_string()
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("command not found: {cmd}"),
        ));
    };

    let status = Command::new(exec).args(args).status()?;
    if !status.success() {
        eprintln!("mattos-init: command exited with {status}: {trimmed}");
    }
    Ok(())
}

#[cfg(unix)]
fn run_rescue_shell() {
    eprintln!("mattos-init: entering emergency rescue shell");
    let stdin = io::stdin();
    loop {
        reap_zombies_nonblocking();
        print!("mattos-rescue# ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => thread::sleep(Duration::from_millis(250)),
            Ok(_) => {
                if let Err(err) = run_command_line(&line) {
                    eprintln!("mattos-init: command failed: {err}");
                }
            }
            Err(err) => {
                eprintln!("mattos-init: stdin read failed: {err}");
                thread::sleep(Duration::from_millis(250));
            }
        }
    }
}

#[cfg(unix)]
fn supervise_brush() {
    match spawn_brush() {
        Ok(mut child) => loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("mattos-init: brush exited with {status}");
                    run_rescue_shell();
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(err) => {
                    eprintln!("mattos-init: brush wait failed: {err}");
                    run_rescue_shell();
                }
            }
        },
        Err(err) => {
            eprintln!("mattos-init: failed to start /bin/brush: {err}");
            run_rescue_shell();
        }
    }
}

#[cfg(unix)]
fn main() {
    let pid = unsafe { libc::getpid() };
    println!("mattos-init: starting as pid {pid}");
    println!("__MATTOS_START__");
    println!("MattOS boot: mounting pseudo-filesystems and launching Brush");

    if pid != 1 {
        eprintln!("mattos-init: warning - not running as PID 1");
    }

    try_mounts();
    supervise_brush();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("mattos-init is Linux/Unix-only and must be built for a Unix target");
    std::process::exit(1);
}
