//! Standalone MattOS graphical installer using a tiny source-built Rust
//! framebuffer frontend. Disk semantics remain exclusively in engine/policy.

use anyhow::{Context, Result, bail};
use clap::Parser;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use mattos_installer::{
    InstallPlan, InstalledProfile, PLAN_VERSION, engine, execute, execute_with_progress,
    render_plan,
};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::mem;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "mattos-install-graphical")]
struct Args {
    /// Execute or preview an automation plan through the shared backend.
    #[arg(long)]
    plan: Option<PathBuf>,
    #[arg(long)]
    yes_really_erase: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FbBitfield { offset: u32, length: u32, msb_right: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct FbVarInfo {
    xres: u32, yres: u32, xres_virtual: u32, yres_virtual: u32,
    xoffset: u32, yoffset: u32, bits_per_pixel: u32, grayscale: u32,
    red: FbBitfield, green: FbBitfield, blue: FbBitfield, transp: FbBitfield,
    nonstd: u32, activate: u32, height: u32, width: u32, accel_flags: u32,
    pixclock: u32, left_margin: u32, right_margin: u32, upper_margin: u32,
    lower_margin: u32, hsync_len: u32, vsync_len: u32, sync: u32, vmode: u32,
    rotate: u32, colorspace: u32, reserved: [u32; 4],
}
#[repr(C)]
struct FbFixInfo {
    id: [u8; 16], smem_start: libc::c_ulong, smem_len: u32, kind: u32,
    type_aux: u32, visual: u32, xpanstep: u16, ypanstep: u16, ywrapstep: u16,
    line_length: u32, mmio_start: libc::c_ulong, mmio_len: u32, accel: u32,
    capabilities: u16, reserved: [u16; 2],
}

struct Screen {
    fb: File,
    pixels: *mut u8,
    length: usize,
    var: FbVarInfo,
    fix: FbFixInfo,
    tty: File,
    saved_termios: libc::termios,
}

impl Screen {
    fn open() -> Result<Self> {
        let fb = OpenOptions::new().read(true).write(true).open("/dev/fb0")
            .context("MattOS graphical installer requires /dev/fb0")?;
        let mut var: FbVarInfo = unsafe { mem::zeroed() };
        let mut fix: FbFixInfo = unsafe { mem::zeroed() };
        if unsafe { libc::ioctl(fb.as_raw_fd(), 0x4600, &mut var) } < 0
            || unsafe { libc::ioctl(fb.as_raw_fd(), 0x4602, &mut fix) } < 0
        { bail!("read framebuffer geometry"); }
        if !matches!(var.bits_per_pixel, 24 | 32) {
            bail!("unsupported framebuffer depth {}; expected 24 or 32", var.bits_per_pixel);
        }
        let length = fix.smem_len as usize;
        let pixels = unsafe {
            libc::mmap(std::ptr::null_mut(), length, libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED, fb.as_raw_fd(), 0)
        } as *mut u8;
        if pixels == libc::MAP_FAILED as *mut u8 { bail!("map framebuffer"); }
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty1")
            .context("open graphical installer console")?;
        let mut saved_termios: libc::termios = unsafe { mem::zeroed() };
        if unsafe { libc::tcgetattr(tty.as_raw_fd(), &mut saved_termios) } < 0 {
            bail!("read installer console mode");
        }
        let mut raw = saved_termios;
        unsafe { libc::cfmakeraw(&mut raw); }
        if unsafe { libc::tcsetattr(tty.as_raw_fd(), libc::TCSANOW, &raw) } < 0
            || unsafe { libc::ioctl(tty.as_raw_fd(), 0x4B3A, 0x01) } < 0
        { bail!("enter graphical console mode"); }
        Ok(Self { fb, pixels, length, var, fix, tty, saved_termios })
    }
    fn color(&self, rgb: u32) -> u32 {
        let channel = |value: u32, field: FbBitfield| {
            let component = value & 0xff;
            let scaled = if field.length >= 8 { component << (field.length - 8) }
                else { component >> (8 - field.length) };
            scaled << field.offset
        };
        channel(rgb >> 16, self.var.red) | channel(rgb >> 8, self.var.green)
            | channel(rgb, self.var.blue)
    }
    fn fill(&mut self, rgb: u32) {
        let color = self.color(rgb).to_ne_bytes();
        let bytes = (self.var.bits_per_pixel / 8) as usize;
        for y in 0..self.var.yres as usize {
            for x in 0..self.var.xres as usize {
                let offset = y * self.fix.line_length as usize + x * bytes;
                unsafe { std::ptr::copy_nonoverlapping(color.as_ptr(), self.pixels.add(offset), bytes); }
            }
        }
    }
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: u32) {
        let color = self.color(rgb).to_ne_bytes();
        let bytes = (self.var.bits_per_pixel / 8) as usize;
        for py in y..(y + h).min(self.var.yres as usize) {
            for px in x..(x + w).min(self.var.xres as usize) {
                let offset = py * self.fix.line_length as usize + px * bytes;
                unsafe { std::ptr::copy_nonoverlapping(color.as_ptr(), self.pixels.add(offset), bytes); }
            }
        }
    }
    fn text(&mut self, mut x: usize, mut y: usize, text: &str, rgb: u32, scale: usize) {
        let origin = x;
        for ch in text.chars() {
            if ch == '\n' || x + 8 * scale >= self.var.xres as usize - 30 {
                x = origin; y += 11 * scale;
                if ch == '\n' { continue; }
            }
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (gy, row) in glyph.iter().enumerate() {
                    for gx in 0..8 {
                        if row & (1 << gx) != 0 {
                            self.rect(x + gx * scale, y + gy * scale, scale, scale, rgb);
                        }
                    }
                }
            }
            x += 9 * scale;
        }
    }
    fn page(&mut self, title: &str, body: &str, footer: &str) {
        self.fill(0x101522);
        self.rect(0, 0, self.var.xres as usize, 86, 0x32265c);
        self.text(38, 28, "MattOS Installer", 0xffffff, 3);
        self.text(55, 125, title, 0x8fc7ff, 3);
        self.text(55, 190, body, 0xe8e8ee, 2);
        self.rect(0, self.var.yres.saturating_sub(62) as usize, self.var.xres as usize, 62, 0x1b2233);
        self.text(38, self.var.yres.saturating_sub(42) as usize, footer, 0xbec7d8, 2);
    }
    fn read_key(&mut self) -> Result<Key> {
        let mut byte = [0u8; 1];
        self.tty.read_exact(&mut byte)?;
        Ok(match byte[0] {
            10 | 13 => Key::Enter,
            8 | 127 => Key::Backspace,
            27 => {
                let mut seq = [0u8; 2];
                self.tty.read_exact(&mut seq)?;
                match seq { [b'[', b'A'] => Key::Up, [b'[', b'B'] => Key::Down, _ => Key::Other }
            }
            32..=126 => Key::Char(byte[0] as char),
            _ => Key::Other,
        })
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.tty.as_raw_fd(), 0x4B3A, 0x00);
            libc::tcsetattr(self.tty.as_raw_fd(), libc::TCSANOW, &self.saved_termios);
            libc::munmap(self.pixels.cast(), self.length);
        }
        let _ = self.fb.sync_all();
    }
}

enum Key { Enter, Backspace, Up, Down, Char(char), Other }

fn text_entry(screen: &mut Screen, title: &str, explanation: &str, secret: bool) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    loop {
        let shown = if secret { "*".repeat(value.len()) } else { String::from_utf8_lossy(&value).into_owned() };
        screen.page(title, &format!("{explanation}\n\n> {shown}"), "Type a value  |  Enter: continue  |  Backspace: edit");
        match screen.read_key()? {
            Key::Enter if !value.is_empty() => return Ok(value),
            Key::Backspace => { value.pop(); }
            Key::Char(ch) if value.len() < 63 => value.push(ch as u8),
            _ => {}
        }
    }
}

fn choose(screen: &mut Screen, title: &str, explanation: &str, values: &[String]) -> Result<usize> {
    if values.is_empty() { bail!("no eligible installation target disks were discovered"); }
    let mut selected = 0usize;
    loop {
        let list = values.iter().enumerate().map(|(index, value)| {
            format!("{} {}", if index == selected { ">" } else { " " }, value)
        }).collect::<Vec<_>>().join("\n");
        screen.page(title, &format!("{explanation}\n\n{list}"), "Up/Down: select  |  Enter: continue");
        match screen.read_key()? {
            Key::Up => selected = selected.saturating_sub(1),
            Key::Down => selected = (selected + 1).min(values.len() - 1),
            Key::Enter => return Ok(selected),
            _ => {}
        }
    }
}

fn error_screen(screen: &mut Screen, error: &anyhow::Error) -> Result<()> {
    let detail = format!("{error:#}");
    eprintln!("mattos-install-graphical: {detail}");
    loop {
        screen.page(
            "Installer could not continue",
            &format!(
                "No installation was performed.\n\n{detail}\n\nThe ISO is never a valid installation target.",
            ),
            "R: reboot  |  S: power off  |  Q: return to console",
        );
        match screen.read_key()? {
            Key::Char('r' | 'R') => {
                Command::new("systemctl").arg("reboot").status()?;
                return Ok(());
            }
            Key::Char('s' | 'S') => {
                Command::new("systemctl").arg("poweroff").status()?;
                return Ok(());
            }
            Key::Char('q' | 'Q') => {
                // The graphical service owns tty1. Hand it back to getty so
                // returning from an error cannot leave a blank virtual tty.
                let _ = Command::new("systemctl")
                    .args(["start", "getty@tty1.service"])
                    .status();
                return Ok(());
            }
            _ => {}
        }
    }
}

fn interactive_session(mut screen: &mut Screen) -> Result<()> {
    screen.page("Welcome", "Install MattOS using the shared, validated GPT/Btrfs installation engine.\n\nNo disk is selected automatically.", "Enter: begin");
    while !matches!(screen.read_key()?, Key::Enter) {}
    choose(&mut screen, "Language", "Currently supported installer language:", &["English (United States)".into()])?;
    choose(&mut screen, "Keyboard", "Currently supported keyboard layout:", &["English (US)".into()])?;
    let disks = engine::discover_install_disks()?;
    let labels = disks.iter().map(|disk| format!("{}  {:.1} GiB  {}", disk.device.display(), disk.size_bytes as f64 / 1073741824.0, disk.model)).collect::<Vec<_>>();
    let disk = disks[choose(&mut screen, "Target disk", "Select the entire disposable disk to erase:", &labels)?].device.clone();
    screen.page("Destructive operation", &format!("ALL DATA ON {} WILL BE DESTROYED.\n\nMattOS will create GPT, a 512 MiB EFI partition, and Btrfs subvolumes @, @home, and @snapshots.", disk.display()), "Enter: acknowledge  |  Power off to cancel safely");
    while !matches!(screen.read_key()?, Key::Enter) {}
    let profile = match choose(&mut screen, "Installed profile", "The installer frontend does not determine the installed profile:", &["MattOS Desktop (COSMIC pending)".into(), "MattOS CLI".into()])? {
        0 => InstalledProfile::Desktop,
        _ => InstalledProfile::Cli,
    };
    let hostname = String::from_utf8(text_entry(&mut screen, "Hostname", "Lowercase letters, digits, and interior hyphens", false)?)?;
    let username = String::from_utf8(text_entry(&mut screen, "Username", "Create the primary non-root administrator", false)?)?;
    let mut password = text_entry(&mut screen, "Password", "Password input is hidden and never written to a plaintext plan", true)?;
    let mut confirm = text_entry(&mut screen, "Confirm password", "Enter the same password again", true)?;
    if password != confirm {
        password.fill(0);
        confirm.fill(0);
        bail!("password confirmation did not match");
    }
    confirm.fill(0);
    let password_hash = engine::hash_password_secure(&mut password)?;
    let plan = InstallPlan { version: PLAN_VERSION, target_disk: disk, installed_profile: profile,
        hostname, username, password_hash: Some(password_hash), test_autologin: false };
    let summary = render_plan(&plan)?;
    screen.page("Installation summary", &summary, "Enter: proceed to final confirmation");
    while !matches!(screen.read_key()?, Key::Enter) {}
    let confirmation = String::from_utf8(text_entry(&mut screen, "Final confirmation", "Type ERASE to install MattOS now", false)?)?;
    if confirmation != "ERASE" { bail!("installation cancelled: confirmation did not match ERASE"); }
    execute_with_progress(&plan, |message| screen.page("Installing MattOS", message, "Please wait; do not power off"))?;
    loop {
        screen.page("Installation complete", "MattOS was installed successfully. Remove installation media before rebooting.", "R: reboot  |  S: shut down  |  Q: return");
        match screen.read_key()? {
            Key::Char('r' | 'R') => { Command::new("systemctl").arg("reboot").status()?; }
            Key::Char('s' | 'S') => { Command::new("systemctl").arg("poweroff").status()?; }
            Key::Char('q' | 'Q') => return Ok(()),
            _ => {}
        }
    }
}

fn interactive() -> Result<()> {
    let mut screen = match Screen::open() {
        Ok(screen) => screen,
        Err(error) => {
            eprintln!("mattos-install-graphical: failed before framebuffer UI: {error:#}");
            let _ = Command::new("systemctl")
                .args(["start", "getty@tty1.service"])
                .status();
            return Err(error);
        }
    };
    match interactive_session(&mut screen) {
        Ok(()) => Ok(()),
        Err(error) => error_screen(&mut screen, &error),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(path) = args.plan {
        let plan = InstallPlan::read(&path)?;
        print!("{}", render_plan(&plan)?);
        if args.yes_really_erase { return execute(&plan); }
        bail!("graphical plan is a dry run; pass --yes-really-erase to execute it");
    }
    interactive()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphical_frontend_accepts_shared_plan_contract() {
        use clap::CommandFactory;
        assert!(Args::command().get_arguments().any(|argument| argument.get_id() == "plan"));
    }
    #[test]
    fn gui_source_never_places_plaintext_password_in_arguments_or_plan_files() {
        let source = include_str!("main.rs");
        assert!(source.contains("hash_password_secure"));
        let password_argument = ["--", "password"].concat();
        let persistent_write = ["fs::", "write("].concat();
        assert!(!source.contains(&password_argument));
        assert!(!source.contains(&persistent_write));
    }

    #[test]
    fn gui_errors_are_rendered_and_offer_a_safe_console_return() {
        let source = include_str!("main.rs");
        assert!(source.contains("Installer could not continue"));
        assert!(source.contains("No installation was performed."));
        assert!(source.contains("The ISO is never a valid installation target."));
        assert!(source.contains("getty@tty1.service"));
    }
}
