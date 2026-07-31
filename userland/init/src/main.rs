#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::io::BufRead;
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
fn mount_fs(source: &str, target: &str, fstype: &str) -> io::Result<()> {
    let source = CString::new(source).expect("valid source CString");
    let target = CString::new(target).expect("valid target CString");
    let fstype = CString::new(fstype).expect("valid fstype CString");

    let rc = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
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

    if let Err(err) = mount_fs("proc", "/proc", "proc") {
        eprintln!("mattos-init: mount /proc failed: {err}");
    }
    if let Err(err) = mount_fs("sysfs", "/sys", "sysfs") {
        eprintln!("mattos-init: mount /sys failed: {err}");
    }
    if let Err(err) = mount_fs("devtmpfs", "/dev", "devtmpfs") {
        eprintln!("mattos-init: mount /dev failed: {err}");
    }
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

    if cmd == "uname" {
        if args == ["-s"] || args.is_empty() {
            println!("Linux");
            return Ok(());
        }
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
fn main() {
    println!("mattos-init: pid1 starting");
    // Emit a stable startup marker for boot-test synchronization.
    println!("__MATTOS_START__");
    try_mounts();

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        match line {
            Ok(line) => {
                if let Err(err) = run_command_line(&line) {
                    eprintln!("mattos-init: command failed: {err}");
                }
            }
            Err(err) => eprintln!("mattos-init: stdin read failed: {err}"),
        }
    }
    eprintln!("mattos-init: stdin closed; idling");

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("mattos-init is Linux/Unix-only and must be built for a Unix target");
    std::process::exit(1);
}
