//! The permanent Rust/libcosmic MattOS installer.
//!
//! This crate deliberately owns only wizard presentation and navigation.  It
//! constructs an `InstallPlan` through `InstallerFrontendModel`, then calls
//! the same policy/engine entry point as `mattos-install`; it never performs
//! disk discovery, partitioning, account creation, or installation itself.

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Length, Limits};
use cosmic::iced::core::text::Wrapping;
use cosmic::iced::futures::{StreamExt, channel::mpsc};
use cosmic::{Application, Element, executor, widget};
use mattos_installer::{InstallProgress, InstallStage, InstalledProfile, RootCredentialPolicy, engine, execute_with_progress};
use mattos_installer::gui_model::{FrontendState, InstallerFrontendModel};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page { Welcome, Locale, Profile, Disk, Storage, Account, Review, Installing, Complete, Error }

#[derive(Clone, Debug)]
enum Message {
    Next, Back, SelectDisk(usize), SelectProfile(InstalledProfile),
    FullName(String), Hostname(String), Username(String), Locale(String), KeyboardLayout(String), KeyboardVariant(String), Timezone(String), KeyboardTest(String), Password(String), PasswordConfirm(String), RootPassword(String), RootPasswordConfirm(String), ToggleAdministrator, ToggleAutologin, ToggleSeparateRoot,
    Install, InstallationEvent(InstallationEvent),
}

#[derive(Clone, Debug)]
enum InstallationEvent { Progress(InstallProgress), Finished(Result<(), String>) }

struct InstallerApp {
    core: Core,
    model: InstallerFrontendModel,
    page: Page,
    password: String,
    password_confirm: String,
    root_password: String, root_password_confirm: String, separate_root: bool,
    error: Option<String>,
    installation_active: bool,
    keyboard_test: String,
}

impl InstallerApp {
    const CONTENT_MAX_WIDTH: f32 = 760.0;

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

    fn fail(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.model.fail(error.clone());
        self.error = Some(error);
        self.page = Page::Error;
    }

    fn start_install(&mut self) -> Task<Message> {
        if self.installation_active || self.page != Page::Review { return Task::none(); }
        let mut password = std::mem::take(&mut self.password).into_bytes();
        clear_secret(&mut self.password_confirm);
        let user_hash = match engine::hash_password_secure(&mut password) {
            Ok(hash) => hash, Err(error) => { self.fail(error.to_string()); return Task::none(); }
        };
        let root = if self.separate_root { let mut root = std::mem::take(&mut self.root_password).into_bytes(); clear_secret(&mut self.root_password_confirm); match engine::hash_password_secure(&mut root) { Ok(hash) => RootCredentialPolicy::SeparatePasswordHash(hash), Err(error) => { self.fail(error.to_string()); return Task::none(); } } } else { RootCredentialPolicy::SameAsUser };
        let plan = match self.model.plan(Some(user_hash), root) {
            Ok(plan) => plan,
            Err(error) => { self.fail(error.to_string()); return Task::none(); }
        };
        self.model.mark_validated();
        self.page = Page::Installing;
        self.installation_active = true;
        let (sender, receiver) = mpsc::unbounded();
        std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let result = execute_with_progress(&plan, |event| {
                // A closed receiver means the UI has gone away. Installation is
                // intentionally not retried or blocked by that lifecycle event.
                let _ = progress_sender.unbounded_send(InstallationEvent::Progress(event));
            });
            drop(progress_sender);
            let _ = sender.unbounded_send(InstallationEvent::Finished(result.map_err(|error| format!("{error:#}"))));
        });
        cosmic::task::stream(receiver.map(|event| cosmic::action::app(Message::InstallationEvent(event))))
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
        // A page may be taller than a low-resolution output. Keep the action
        // row outside its scroll region so Back/Continue/Install are always
        // reachable; libcosmic lays this out in logical Wayland pixels and
        // applies the compositor's output scale for us.
        let page = widget::container(content)
            .width(Length::Fill)
            .max_width(Self::CONTENT_MAX_WIDTH)
            .padding([16, 20])
            .center_x(Length::Fill);
        let footer = widget::container(row)
            .width(Length::Fill)
            .padding([12, 20]);
        widget::column::with_capacity(2)
            .push(widget::scrollable(page).width(Length::Fill).height(Length::Fill))
            .push(footer)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.page {
            Page::Welcome => widget::column::with_capacity(3).spacing(16).width(Length::Fill)
                .push(widget::text::title2("Welcome to the MattOS installer"))
                .push(wrapped_body("This wizard installs MattOS to one disk you explicitly choose. That disk will be erased."))
                .push(wrapped_body("You can return at any point before the final confirmation.")).into(),
            Page::Locale => widget::column::with_capacity(11).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Language and keyboard"))
                .push(wrapped_body("Use canonical identifiers from the offline MattOS locale, XKB, and zoneinfo databases. Invalid selections are rejected before disk changes."))
                .push(widget::text::body("Locale")).push(widget::text_input::text_input("en_US.UTF-8", &self.model.locale).width(Length::Fill).on_input(Message::Locale))
                .push(widget::text::body("Keyboard layout")).push(widget::text_input::text_input("us", &self.model.keyboard_layout).width(Length::Fill).on_input(Message::KeyboardLayout))
                .push(widget::text::body("Keyboard variant (optional)")).push(widget::text_input::text_input("Default", &self.model.keyboard_variant).width(Length::Fill).on_input(Message::KeyboardVariant))
                .push(widget::text::body("Test your keyboard")).push(widget::text_input::text_input("Type here to verify the selected layout", &self.keyboard_test).width(Length::Fill).on_input(Message::KeyboardTest))
                .push(widget::text::body("Timezone")).push(widget::text_input::text_input("Etc/UTC", &self.model.timezone).width(Length::Fill).on_input(Message::Timezone)).into(),
            Page::Profile => widget::column::with_capacity(4).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Choose the installed MattOS profile"))
                .push(profile_card("MattOS Desktop", "Graphical MattOS environment using COSMIC.", self.model.installed_profile == InstalledProfile::Desktop, InstalledProfile::Desktop))
                .push(profile_card("MattOS CLI", "Non-graphical MattOS base.", self.model.installed_profile == InstalledProfile::Cli, InstalledProfile::Cli)).into(),
            Page::Disk => {
                let mut content = widget::column::with_capacity(self.model.disks.len() + 3).spacing(12).width(Length::Fill)
                    .push(widget::text::title2("Select a target disk"))
                    .push(wrapped_body("The selected writable disk will be completely erased. Optical and read-only devices are never eligible."));
                if self.model.disks.is_empty() { content = content.push(wrapped_body("No eligible writable installation disks were found.")); }
                for (index, disk) in self.model.disks.iter().enumerate() {
                    let selected = self.model.selected_disk.as_ref() == Some(&disk.device);
                    content = content.push(disk_card(index, disk, selected));
                }
                content.into()
            }
            Page::Storage => widget::column::with_capacity(8).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Automatic storage layout"))
                .push(wrapped_body("The installer erases the whole selected disk and creates a GPT partition table."))
                .push(wrapped_body("512 MiB FAT32 EFI System Partition"))
                .push(wrapped_body("Btrfs system partition with subvolumes: @ → /, @home → /home, and @snapshots → /.snapshots."))
                .push(wrapped_body("Manual partitioning is not part of this installer. The selected disk will be erased.")).into(),
            Page::Account => widget::column::with_capacity(14).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Set up the installed system"))
                .push(widget::text::body("Full name")).push(widget::text_input::text_input("Full name", &self.model.full_name).width(Length::Fill).on_input(Message::FullName))
                .push(widget::text::body("Computer name")).push(widget::text_input::text_input("Hostname", &self.model.hostname).width(Length::Fill).on_input(Message::Hostname))
                .push(widget::text::body("Username"))
                .push(widget::text_input::text_input("Username", &self.model.username).width(Length::Fill).on_input(Message::Username))
                .push(widget::button::standard(if self.model.administrator { "Account type: Administrator" } else { "Account type: Standard user" }).on_press(Message::ToggleAdministrator))
                .push(widget::button::standard(if self.model.automatic_login { "Automatic login: enabled" } else { "Automatic login: disabled" }).on_press(Message::ToggleAutologin))
                .push(widget::text_input::secure_input("Password", &self.password, None, true).width(Length::Fill).on_input(Message::Password))
                .push(widget::text_input::secure_input("Confirm password", &self.password_confirm, None, true).width(Length::Fill).on_input(Message::PasswordConfirm))
                .push(widget::button::standard(if self.separate_root { "Use a different root password: enabled" } else { "Root uses the user password" }).on_press(Message::ToggleSeparateRoot))
                .push(root_fields(self))
                .push(wrapped_body("The password is hashed in memory immediately before installation. Plaintext is never written to a plan, log, or command line.")).into(),
            Page::Review => {
                let disk = self.model.selected_disk.as_ref().and_then(|selected| self.model.disks.iter().find(|disk| &disk.device == selected));
                let disk_description = disk.map(|disk| format!("{} — {} — {:.1} GiB", disk.device.display(), disk.model, gibibytes(disk.size_bytes))).unwrap_or_else(|| "No target disk selected.".into());
                widget::column::with_capacity(8).spacing(12).width(Length::Fill)
                    .push(widget::text::title2("Review and confirm installation"))
                    .push(wrapped_body("WARNING: the selected target disk will be completely erased."))
                    .push(widget::text::heading("Target disk"))
                    .push(wrapped_body(disk_description))
                    .push(widget::text::heading("Installation"))
                    .push(wrapped_body(format!("Full name: {}. Username: {}. Computer: {}. Account type: {}. Automatic login: {}. Root: {}.", self.model.full_name, self.model.username, self.model.hostname, if self.model.administrator { "Administrator" } else { "Standard user" }, if self.model.automatic_login { "Enabled" } else { "Disabled" }, if self.separate_root { "separate password configured" } else { "uses the user password" })))
                    .push(widget::text::heading("Region and input"))
                    .push(wrapped_body(format!("Locale: {}. Keyboard: {}. Variant: {}. Timezone: {}.", self.model.locale, self.model.keyboard_layout, if self.model.keyboard_variant.is_empty() { "Default" } else { &self.model.keyboard_variant }, self.model.timezone)))
                    .push(widget::text::heading("Boot and filesystem"))
                    .push(wrapped_body("UEFI boot; GPT; 512 MiB FAT32 EFI System Partition; Btrfs subvolumes @ → /, @home → /home, and @snapshots → /.snapshots.")).into()
            }
            Page::Installing => {
                let (event, stage, detail) = match &self.model.state {
                    FrontendState::Installing(event) => (Some(event), event.stage.display_name(), event.detail.as_str()),
                    _ => (None, "Preparing", "Starting shared MattOS installer engine…"),
                };
                let completed = event.map_or(0, |event| event.completed_stages);
                let total = event.map_or(mattos_installer::policy::INSTALL_STAGE_COUNT, |event| event.total_stages);
                let fraction = event.map_or(0.0, InstallProgress::fraction);
                let mut stages = widget::column::with_capacity(InstallStage::ALL.len()).spacing(6).width(Length::Fill);
                for (index, listed_stage) in InstallStage::ALL.iter().enumerate() {
                    let status = if event.is_some_and(|event| event.stage == *listed_stage && event.stage != InstallStage::Complete) { "●" } else if index < completed { "✓" } else { "○" };
                    stages = stages.push(wrapped_body(format!("{status} {}", listed_stage.display_name())));
                }
                widget::column::with_capacity(7).spacing(16).width(Length::Fill)
                    .push(widget::text::title2("Installing MattOS"))
                    .push(wrapped_body(format!("{completed} / {total}")))
                    .push(widget::determinate_linear(fraction).width(Length::Fill).girth(Length::Fixed(8.0)))
                    .push(widget::text::heading(stage))
                    .push(wrapped_body(detail))
                    .push(stages)
                    .push(wrapped_body("Do not power off the computer while MattOS is being installed.")).into()
            }
            Page::Complete => widget::column::with_capacity(2).spacing(16).width(Length::Fill).push(widget::text::title2("MattOS installation complete")).push(wrapped_body("Remove the installation media and reboot into the installed system.")).into(),
            Page::Error => widget::column::with_capacity(3).spacing(16).width(Length::Fill).push(widget::text::title2("Installation needs attention")).push(wrapped_body(self.error.as_deref().unwrap_or("Unknown installer error"))).push(wrapped_body("No error is hidden: return to an earlier page, correct the problem, and try again.")).into(),
        }
    }
}

fn wrapped_body<'a>(text: impl Into<String>) -> Element<'a, Message> {
    widget::text::body(text.into()).width(Length::Fill).wrapping(Wrapping::Word).into()
}

fn root_fields(app: &InstallerApp) -> Element<'_, Message> {
    if app.separate_root {
        widget::column::with_capacity(2).spacing(8)
            .push(widget::text_input::secure_input("Root password", &app.root_password, None, true).width(Length::Fill).on_input(Message::RootPassword))
            .push(widget::text_input::secure_input("Confirm root password", &app.root_password_confirm, None, true).width(Length::Fill).on_input(Message::RootPasswordConfirm))
            .into()
    } else { widget::container(widget::space::vertical().height(0)).into() }
}

fn gibibytes(bytes: u64) -> f64 { bytes as f64 / 1_073_741_824.0 }

fn profile_card(profile: &'static str, description: &'static str, selected: bool, value: InstalledProfile) -> Element<'static, Message> {
    let label = if selected { format!("{profile} — selected") } else { profile.into() };
    widget::button::custom(
        widget::column::with_capacity(2).spacing(4).width(Length::Fill)
            .push(widget::text::heading(label).wrapping(Wrapping::Word))
            .push(widget::text::caption(description).width(Length::Fill).wrapping(Wrapping::Word)),
    ).width(Length::Fill).padding(14).on_press(Message::SelectProfile(value)).into()
}

fn disk_card(index: usize, disk: &mattos_installer::engine::InstallDisk, selected: bool) -> Element<'static, Message> {
    let selected = if selected { "Selected target" } else { "Select this disk" };
    widget::button::custom(
        widget::column::with_capacity(3).spacing(4).width(Length::Fill)
            .push(widget::text::heading(disk.device.display().to_string()).wrapping(Wrapping::Word))
            .push(widget::text::body(disk.model.clone()).width(Length::Fill).wrapping(Wrapping::Word))
            .push(widget::text::caption(format!("{:.1} GiB — {selected}", gibibytes(disk.size_bytes))).width(Length::Fill).wrapping(Wrapping::Word)),
    ).width(Length::Fill).padding(14).on_press(Message::SelectDisk(index)).into()
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
    fn init(core: Core, model: Self::Flags) -> (Self, Task<Message>) { (Self { core, model, page: Page::Welcome, password: String::new(), password_confirm: String::new(), root_password: String::new(), root_password_confirm: String::new(), separate_root: false, error: None, installation_active: false, keyboard_test: String::new() }, Task::none()) }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => self.next(), Message::Back => self.back(),
            Message::SelectDisk(index) => if let Err(error) = self.model.select_disk(index) { self.fail(error.to_string()); },
            Message::SelectProfile(profile) => self.model.installed_profile = profile,
            Message::FullName(value) => self.model.full_name = value, Message::Hostname(value) => self.model.hostname = value, Message::Username(value) => self.model.username = value,
            Message::Locale(value) => self.model.locale = value, Message::KeyboardLayout(value) => self.model.keyboard_layout = value, Message::KeyboardVariant(value) => self.model.keyboard_variant = value, Message::Timezone(value) => self.model.timezone = value, Message::KeyboardTest(value) => self.keyboard_test = value,
            Message::Password(value) => self.password = value, Message::PasswordConfirm(value) => self.password_confirm = value,
            Message::RootPassword(value) => self.root_password = value, Message::RootPasswordConfirm(value) => self.root_password_confirm = value,
            Message::ToggleAdministrator => self.model.administrator = !self.model.administrator, Message::ToggleAutologin => self.model.automatic_login = !self.model.automatic_login,
            Message::ToggleSeparateRoot => { self.separate_root = !self.separate_root; if !self.separate_root { clear_secret(&mut self.root_password); clear_secret(&mut self.root_password_confirm); } },
            Message::Install => return self.start_install(),
            Message::InstallationEvent(InstallationEvent::Progress(event)) => {
                if self.installation_active && self.page == Page::Installing { self.model.progress(event); }
            }
            Message::InstallationEvent(InstallationEvent::Finished(result)) => {
                if !self.installation_active || self.page != Page::Installing { return Task::none(); }
                self.installation_active = false;
                match result { Ok(()) => { self.model.complete(); self.page = Page::Complete; }, Err(error) => self.fail(error) }
            }
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
    // libcosmic defaults to a normal 1024×768 logical-pixel window and
    // applies the Wayland output scale. The minimum merely keeps the wizard
    // usable on firmware/KMS fallback modes; content scrolls below it.
    cosmic::app::run::<InstallerApp>(
        Settings::default().size_limits(Limits::NONE.min_width(480.0).min_height(400.0)),
        InstallerFrontendModel::discover()?,
    )?;
    Ok(())
}
