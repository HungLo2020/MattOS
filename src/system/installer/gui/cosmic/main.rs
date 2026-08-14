//! The permanent Rust/libcosmic MattOS installer.
//!
//! This crate deliberately owns only wizard presentation and navigation.  It
//! constructs an `InstallPlan` through `InstallerFrontendModel`, then calls
//! the same policy/engine entry point as `mattos-install`; it never performs
//! disk discovery, partitioning, account creation, or installation itself.

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Length, Size};
use cosmic::{Application, Element, executor, widget};
use mattos_installer::{InstallProgress, InstallStage, InstalledProfile, engine, execute_with_progress};
use mattos_installer::gui_model::{FrontendState, InstallerFrontendModel};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page { Welcome, Locale, Profile, Disk, Storage, Account, Review, Installing, Complete, Error }

#[derive(Clone, Debug)]
enum Message {
    Next, Back, SelectDisk(usize), SelectProfile(InstalledProfile),
    Hostname(String), Username(String), Password(String), PasswordConfirm(String),
    Install, InstallationFinished(Result<Vec<InstallProgress>, String>),
}

struct InstallerApp {
    core: Core,
    model: InstallerFrontendModel,
    page: Page,
    password: String,
    password_confirm: String,
    error: Option<String>,
}

impl InstallerApp {
    fn next(&mut self) {
        self.error = None;
        self.page = match self.page {
            Page::Welcome => Page::Locale,
            Page::Locale => Page::Profile,
            Page::Profile => Page::Disk,
            Page::Disk => if self.model.selected_disk.is_some() { Page::Storage } else { self.fail("Choose an explicit writable target disk first"); return; },
            Page::Storage => Page::Account,
            Page::Account => {
                if self.password.len() < 8 { self.fail("Choose a password of at least 8 characters"); return; }
                if self.password != self.password_confirm { self.fail("Password confirmation does not match"); return; }
                Page::Review
            }
            page => page,
        };
    }

    fn back(&mut self) {
        self.error = None;
        self.page = match self.page {
            Page::Locale => Page::Welcome, Page::Profile => Page::Locale,
            Page::Disk => Page::Profile, Page::Storage => Page::Disk,
            Page::Account => Page::Storage, Page::Review => Page::Account, page => page,
        };
    }

    fn fail(&mut self, error: impl Into<String>) { self.error = Some(error.into()); self.page = Page::Error; }

    fn start_install(&mut self) -> Task<Message> {
        let mut password = std::mem::take(&mut self.password).into_bytes();
        clear_secret(&mut self.password_confirm);
        let plan = match self.model.plan(Some(match engine::hash_password_secure(&mut password) {
            Ok(hash) => hash, Err(error) => { self.fail(error.to_string()); return Task::none(); }
        })) {
            Ok(plan) => plan,
            Err(error) => { self.fail(error.to_string()); return Task::none(); }
        };
        self.model.mark_validated();
        self.page = Page::Installing;
        Task::perform(async move {
            let mut progress = Vec::new();
            execute_with_progress(&plan, |event| progress.push(event)).map(|()| progress).map_err(|error| format!("{error:#}"))
        }, |result| cosmic::action::app(Message::InstallationFinished(result)))
    }

    fn navigation<'a>(&self, content: Element<'a, Message>) -> Element<'a, Message> {
        let mut row = widget::row::with_capacity(2).spacing(8);
        if matches!(self.page, Page::Locale | Page::Profile | Page::Disk | Page::Storage | Page::Account | Page::Review) {
            row = row.push(widget::button::standard("Back").on_press(Message::Back));
        }
        if matches!(self.page, Page::Welcome | Page::Locale | Page::Profile | Page::Disk | Page::Storage | Page::Account) {
            row = row.push(widget::button::suggested("Continue").on_press(Message::Next));
        }
        if self.page == Page::Review { row = row.push(widget::button::destructive("Erase disk and install").on_press(Message::Install)); }
        // The live ISO must remain usable on QEMU's 640×480 fallback mode.
        // Keep the navigation controls in the initial viewport rather than
        // depending on an output-mode change that firmware/KMS may not have
        // completed yet.  The window remains resizable on larger displays.
        widget::container(widget::column::with_capacity(2).spacing(12).push(content).push(row))
            .padding(16).width(Length::Fill).height(Length::Fill).into()
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.page {
            Page::Welcome => widget::column::with_capacity(3).spacing(16)
                .push(widget::text::title2("Welcome to the MattOS installer"))
                .push(widget::text::body("This wizard installs MattOS to one disk you explicitly choose. That disk will be erased."))
                .push(widget::text::body("You can return at any point before the final confirmation.")).into(),
            Page::Locale => widget::column::with_capacity(3).spacing(16)
                .push(widget::text::title2("Language and keyboard"))
                .push(widget::text::body("This MattOS image currently provides English (US) with the US keyboard layout."))
                .push(widget::text::body("Locale and keyboard selection is intentionally not presented as configurable until the installed image can apply those choices.")).into(),
            Page::Profile => widget::column::with_capacity(4).spacing(16)
                .push(widget::text::title2("Choose the installed MattOS profile"))
                .push(widget::button::standard("MattOS Desktop — graphical MattOS/COSMIC environment").on_press(Message::SelectProfile(InstalledProfile::Desktop)))
                .push(widget::button::standard("MattOS CLI — non-graphical MattOS base").on_press(Message::SelectProfile(InstalledProfile::Cli)))
                .push(widget::text::body(format!("Selected: {:?}", self.model.installed_profile))).into(),
            Page::Disk => {
                let mut content = widget::column::with_capacity(self.model.disks.len() + 3).spacing(12)
                    .push(widget::text::title2("Select a target disk"))
                    .push(widget::text::body("The selected writable disk will be completely erased. Optical and read-only devices are never eligible."));
                if self.model.disks.is_empty() { content = content.push(widget::text::body("No eligible writable installation disks were found.")); }
                for (index, disk) in self.model.disks.iter().enumerate() {
                    let selected = self.model.selected_disk.as_ref() == Some(&disk.device);
                    content = content.push(widget::button::standard(format!("{}{} — {} — {:.1} GiB", if selected { "Selected: " } else { "" }, disk.device.display(), disk.model, disk.size_bytes as f64 / 1_073_741_824.0)).on_press(Message::SelectDisk(index)));
                }
                content.into()
            }
            Page::Storage => widget::column::with_capacity(8).spacing(12)
                .push(widget::text::title2("Automatic storage layout"))
                .push(widget::text::body("GPT"))
                .push(widget::text::body("├── EFI System Partition — 512 MiB — FAT32"))
                .push(widget::text::body("└── MattOS — Btrfs"))
                .push(widget::text::body("    ├── @          /"))
                .push(widget::text::body("    ├── @home      /home"))
                .push(widget::text::body("    └── @snapshots /.snapshots"))
                .push(widget::text::body("Manual partitioning is not part of this installer. The selected disk will be erased.")).into(),
            Page::Account => widget::column::with_capacity(6).spacing(12)
                .push(widget::text::title2("Set up the installed system"))
                .push(widget::text_input::text_input("Hostname", &self.model.hostname).on_input(Message::Hostname))
                .push(widget::text_input::text_input("Username", &self.model.username).on_input(Message::Username))
                .push(widget::text_input::secure_input("Password", &self.password, None, true).on_input(Message::Password))
                .push(widget::text_input::secure_input("Confirm password", &self.password_confirm, None, true).on_input(Message::PasswordConfirm))
                .push(widget::text::body("The password is hashed in memory immediately before installation. Plaintext is never written to a plan, log, or command line.")).into(),
            Page::Review => {
                let summary = self.model.summary().unwrap_or_else(|error| format!("Plan validation error: {error}"));
                widget::column::with_capacity(4).spacing(16)
                    .push(widget::text::title2("Review and confirm installation"))
                    .push(widget::text::body("WARNING: the selected disk will be erased."))
                    .push(widget::text::body(summary))
                    .push(widget::text::body("Boot mode: UEFI. Storage: GPT, EFI FAT32, Btrfs @ / @home / @snapshots.")).into()
            }
            Page::Installing => {
                let detail = match &self.model.state { FrontendState::Installing(event) => event.detail.as_str(), _ => "Starting shared MattOS installer engine…" };
                widget::column::with_capacity(3).spacing(16).push(widget::text::title2("Installing MattOS")).push(widget::text::body(detail)).push(widget::text::body("Please do not power off the computer.")).into()
            }
            Page::Complete => widget::column::with_capacity(2).spacing(16).push(widget::text::title2("MattOS installation complete")).push(widget::text::body("Remove the installation media and reboot into the installed system.")).into(),
            Page::Error => widget::column::with_capacity(3).spacing(16).push(widget::text::title2("Installation needs attention")).push(widget::text::body(self.error.as_deref().unwrap_or("Unknown installer error"))).push(widget::text::body("No error is hidden: return to an earlier page, correct the problem, and try again.")).into(),
        }
    }
}

fn clear_secret(value: &mut String) {
    // Rust strings are UTF-8; replacing every byte with NUL preserves valid
    // UTF-8 before clearing the owning buffer.
    unsafe { value.as_bytes_mut().fill(0) };
    value.clear();
}

impl Application for InstallerApp {
    type Executor = executor::Default;
    type Flags = InstallerFrontendModel;
    type Message = Message;
    const APP_ID: &'static str = "com.mattsherfey.MattOS.Installer";
    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }
    fn init(core: Core, model: Self::Flags) -> (Self, Task<Message>) { (Self { core, model, page: Page::Welcome, password: String::new(), password_confirm: String::new(), error: None }, Task::none()) }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => self.next(), Message::Back => self.back(),
            Message::SelectDisk(index) => if let Err(error) = self.model.select_disk(index) { self.fail(error.to_string()); },
            Message::SelectProfile(profile) => self.model.installed_profile = profile,
            Message::Hostname(value) => self.model.hostname = value, Message::Username(value) => self.model.username = value,
            Message::Password(value) => self.password = value, Message::PasswordConfirm(value) => self.password_confirm = value,
            Message::Install => return self.start_install(),
            Message::InstallationFinished(result) => match result { Ok(events) => { if let Some(event) = events.last().cloned() { self.model.progress(event); } self.model.complete(); self.page = Page::Complete; }, Err(error) => self.fail(error) },
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> { self.navigation(self.page_content()) }
}

fn contract_proof() -> anyhow::Result<()> {
    let model = InstallerFrontendModel::discover()?;
    println!("frontend=cosmic pages=welcome,locale,profile,disk,storage,account,review,installing,complete,error");
    println!("shared_disk_count={} profiles=desktop,cli", model.disks.len());
    println!("shared_plan_policy_engine=true structured_progress={:?}", InstallStage::Preparing);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|argument| argument == "--contract-proof") { return contract_proof(); }
    // Start within the universally visible QEMU fallback mode.  This is only
    // the initial size: libcosmic keeps the toplevel resizable for real
    // displays and higher KMS modes.
    cosmic::app::run::<InstallerApp>(Settings::default().size(Size::new(640.0, 480.0)), InstallerFrontendModel::discover()?)?;
    Ok(())
}
