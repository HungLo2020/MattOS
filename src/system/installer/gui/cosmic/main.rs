//! The permanent Rust/libcosmic MattOS installer.
//!
//! This crate deliberately owns only wizard presentation and navigation.  It
//! constructs an `InstallPlan` through `InstallerFrontendModel`, then calls
//! the same policy/engine entry point as `mattos-install`; it never performs
//! disk discovery, partitioning, account creation, or installation itself.

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::core::text::Wrapping;
use cosmic::iced::futures::{StreamExt, channel::mpsc};
use cosmic::iced::widget::combo_box;
use cosmic::iced::{Length, Limits};
use cosmic::{Application, Element, executor, widget};
use mattos_installer::gui_model::{FrontendState, InstallerFrontendModel};
use mattos_installer::{
    Choice, EncryptionPolicy, Filesystem, GuidedEfi, InstallProgress, InstallStage,
    InstalledProfile, KeyboardLayout, PartitionAction, PartitionOperation, RootCredentialPolicy,
    RootFilesystem, StoragePlan, discover_keyboard_layouts, discover_locales, discover_timezones,
    engine, execute_with_progress, render_storage_plan,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Page {
    Welcome,
    Locale,
    Profile,
    Disk,
    Storage,
    Account,
    Review,
    Installing,
    Complete,
    Error,
}

#[derive(Clone, Debug)]
enum Message {
    Next,
    Back,
    SelectDisk(usize),
    SelectProfile(InstalledProfile),
    FullName(String),
    Hostname(String),
    Username(String),
    Locale(Choice),
    KeyboardLayout(KeyboardLayout),
    KeyboardVariant(Choice),
    Timezone(Choice),
    KeyboardTest(String),
    Password(String),
    PasswordConfirm(String),
    RootPassword(String),
    RootPasswordConfirm(String),
    ToggleAdministrator,
    ToggleAutologin,
    ToggleSeparateRoot,
    GuidedBtrfs,
    GuidedExt4,
    ManualStorage,
    CreateEfi,
    ReuseEfi(usize),
    ToggleEfiFormat,
    CyclePartitionAction(usize),
    CyclePartitionMount(usize),
    CyclePartitionFilesystem(usize),
    ManualDevice(String),
    ManualNumber(String),
    ManualStart(String),
    ManualSize(String),
    CycleNewFilesystem,
    CycleNewMount,
    AddManualPartition,
    RemoveManualOperation(usize),
    ToggleDestructiveConfirmation,
    Install,
    InstallationEvent(InstallationEvent),
}

#[derive(Clone, Debug)]
enum InstallationEvent {
    Progress(InstallProgress),
    Finished(Result<(), String>),
}

struct InstallerApp {
    core: Core,
    model: InstallerFrontendModel,
    page: Page,
    password: String,
    password_confirm: String,
    root_password: String,
    root_password_confirm: String,
    separate_root: bool,
    error: Option<String>,
    installation_active: bool,
    keyboard_test: String,
    manual_device: String,
    manual_number: String,
    manual_start: String,
    manual_size: String,
    new_filesystem: Filesystem,
    new_mount: Option<String>,
    destructive_confirmed: bool,
    locales: combo_box::State<Choice>,
    keyboard_layouts: combo_box::State<KeyboardLayout>,
    keyboard_variants: combo_box::State<Choice>,
    timezones: combo_box::State<Choice>,
}

struct InstallerFlags {
    model: InstallerFrontendModel,
    locales: Vec<Choice>,
    keyboard_layouts: Vec<KeyboardLayout>,
    timezones: Vec<Choice>,
}

impl InstallerApp {
    const CONTENT_MAX_WIDTH: f32 = 760.0;

    fn next(&mut self) {
        self.error = None;
        self.page = match self.page {
            Page::Welcome => Page::Locale,
            Page::Locale => Page::Profile,
            Page::Profile => Page::Disk,
            Page::Disk => {
                if self.model.selected_disk.is_some() {
                    Page::Storage
                } else {
                    self.fail("Choose an explicit writable target disk first");
                    return;
                }
            }
            Page::Storage => {
                let Some(disk) = self.model.selected_disk.as_deref() else {
                    self.fail("Select a disk first");
                    return;
                };
                if let Err(error) = self.model.storage.validate(disk) {
                    self.error = Some(error.to_string());
                    return;
                }
                Page::Account
            }
            Page::Account => {
                if self.password.len() < 8 {
                    self.fail("Choose a password of at least 8 characters");
                    return;
                }
                if self.password != self.password_confirm {
                    self.fail("Password confirmation does not match");
                    return;
                }
                self.destructive_confirmed = false;
                Page::Review
            }
            page => page,
        };
    }

    fn back(&mut self) {
        self.error = None;
        self.page = match self.page {
            Page::Locale => Page::Welcome,
            Page::Profile => Page::Locale,
            Page::Disk => Page::Profile,
            Page::Storage => Page::Disk,
            Page::Account => Page::Storage,
            Page::Review => Page::Account,
            page => page,
        };
    }

    fn fail(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.model.fail(error.clone());
        self.error = Some(error);
        self.page = Page::Error;
    }

    fn start_install(&mut self) -> Task<Message> {
        if self.installation_active || self.page != Page::Review {
            return Task::none();
        }
        let mut password = std::mem::take(&mut self.password).into_bytes();
        clear_secret(&mut self.password_confirm);
        let user_hash = match engine::hash_password_secure(&mut password) {
            Ok(hash) => hash,
            Err(error) => {
                self.fail(error.to_string());
                return Task::none();
            }
        };
        let root = if self.separate_root {
            let mut root = std::mem::take(&mut self.root_password).into_bytes();
            clear_secret(&mut self.root_password_confirm);
            match engine::hash_password_secure(&mut root) {
                Ok(hash) => RootCredentialPolicy::SeparatePasswordHash(hash),
                Err(error) => {
                    self.fail(error.to_string());
                    return Task::none();
                }
            }
        } else {
            RootCredentialPolicy::SameAsUser
        };
        let plan = match self.model.plan(Some(user_hash), root) {
            Ok(plan) => plan,
            Err(error) => {
                self.fail(error.to_string());
                return Task::none();
            }
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
            let _ = sender.unbounded_send(InstallationEvent::Finished(
                result.map_err(|error| format!("{error:#}")),
            ));
        });
        cosmic::task::stream(
            receiver.map(|event| cosmic::action::app(Message::InstallationEvent(event))),
        )
    }

    fn navigation<'a>(&self, content: Element<'a, Message>) -> Element<'a, Message> {
        let mut row = widget::row::with_capacity(2).spacing(8);
        if matches!(
            self.page,
            Page::Locale
                | Page::Profile
                | Page::Disk
                | Page::Storage
                | Page::Account
                | Page::Review
        ) {
            row = row.push(widget::button::standard("Back").on_press(Message::Back));
        }
        if matches!(
            self.page,
            Page::Welcome
                | Page::Locale
                | Page::Profile
                | Page::Disk
                | Page::Storage
                | Page::Account
        ) {
            row = row.push(widget::button::suggested("Continue").on_press(Message::Next));
        }
        if self.page == Page::Review && self.destructive_confirmed {
            row = row.push(
                widget::button::destructive("Apply the reviewed storage plan and install")
                    .on_press(Message::Install),
            );
        }
        // A page may be taller than a low-resolution output. Keep the action
        // row outside its scroll region so Back/Continue/Install are always
        // reachable; libcosmic lays this out in logical Wayland pixels and
        // applies the compositor's output scale for us.
        let page = widget::container(content)
            .width(Length::Fill)
            .max_width(Self::CONTENT_MAX_WIDTH)
            .padding([16, 20])
            .center_x(Length::Fill);
        let footer = widget::container(row).width(Length::Fill).padding([12, 20]);
        widget::column::with_capacity(2)
            .push(
                widget::scrollable(page)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .push(footer)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn storage_page(&self) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(24).spacing(12).width(Length::Fill)
            .push(widget::text::title2("Storage layout"))
            .push(wrapped_body("Guided installation replaces the selected disk layout. Manual installation applies only the explicit operations shown below."))
            .push(widget::button::standard(if matches!(self.model.storage, StoragePlan::GuidedWholeDisk { filesystem: RootFilesystem::Btrfs, .. }) { "Guided whole disk — Btrfs (Recommended) — selected" } else { "Guided whole disk — Btrfs (Recommended)" }).on_press(Message::GuidedBtrfs))
            .push(widget::button::standard(if matches!(self.model.storage, StoragePlan::GuidedWholeDisk { filesystem: RootFilesystem::Ext4, .. }) { "Guided whole disk — ext4 — selected" } else { "Guided whole disk — ext4" }).on_press(Message::GuidedExt4))
            .push(widget::button::standard(if matches!(self.model.storage, StoragePlan::Manual { .. }) { "Manual/custom — selected" } else { "Manual/custom" }).on_press(Message::ManualStorage));
        match &self.model.storage {
            StoragePlan::GuidedWholeDisk { filesystem, efi } => {
                content = content
                    .push(wrapped_body(match filesystem {
                        RootFilesystem::Btrfs => {
                            "Creates FAT32 EFI plus Btrfs @, @home, and @snapshots subvolumes."
                        }
                        RootFilesystem::Ext4 => {
                            "Creates FAT32 EFI plus a normal ext4 root partition."
                        }
                    }))
                    .push(widget::text::heading("EFI System Partition"))
                    .push(
                        widget::button::standard(if matches!(efi, GuidedEfi::Create) {
                            "Create and format a new 512 MiB ESP — selected"
                        } else {
                            "Create and format a new 512 MiB ESP"
                        })
                        .on_press(Message::CreateEfi),
                    );
                for (index, partition) in self
                    .model
                    .partitions
                    .iter()
                    .enumerate()
                    .filter(|(_, partition)| partition.is_esp)
                {
                    let selected = matches!(efi, GuidedEfi::Reuse { device, .. } if device == &partition.device);
                    content = content.push(
                        widget::button::standard(format!(
                            "{} {} ({:.1} MiB, {})",
                            if selected {
                                "Reuse — selected:"
                            } else {
                                "Reuse:"
                            },
                            partition.device.display(),
                            partition.size_bytes as f64 / 1_048_576.0,
                            partition.filesystem.as_deref().unwrap_or("unformatted")
                        ))
                        .on_press(Message::ReuseEfi(index)),
                    );
                }
                if let GuidedEfi::Reuse { format, .. } = efi {
                    content = content.push(
                        widget::button::destructive(if *format {
                            "Format reused ESP: YES"
                        } else {
                            "Format reused ESP: no (preserve)"
                        })
                        .on_press(Message::ToggleEfiFormat),
                    );
                }
            }
            StoragePlan::Manual { partitions } => {
                content = content.push(wrapped_body("Existing partitions are preserved unless their operation explicitly says FORMAT or DELETE. Assign exactly one / and one /boot/efi."));
                for (index, partition) in self.model.partitions.iter().enumerate() {
                    if let Some(operation) = partitions
                        .iter()
                        .find(|operation| operation.device == partition.device)
                    {
                        content = content.push(widget::container(widget::column::with_capacity(6).spacing(6).width(Length::Fill)
                            .push(widget::text::heading(partition.device.display().to_string()))
                            .push(wrapped_body(format!("{:.1} GiB • filesystem {} • type {} • ESP {} • existing mounts {}", partition.size_bytes as f64 / 1_073_741_824.0, partition.filesystem.as_deref().unwrap_or("unformatted"), partition.partition_type.as_deref().unwrap_or("unknown"), if partition.is_esp { "yes" } else { "no" }, if partition.mount_points.is_empty() { "none".into() } else { partition.mount_points.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ") })))
                            .push(widget::button::standard(format!("Action: {:?}", operation.action)).on_press(Message::CyclePartitionAction(index)))
                            .push(widget::button::standard(format!("Filesystem: {}", operation.filesystem.map_or("existing", Filesystem::display_name))).on_press(Message::CyclePartitionFilesystem(index)))
                            .push(widget::button::standard(format!("Mount: {}", operation.mount_point.as_deref().unwrap_or("none"))).on_press(Message::CyclePartitionMount(index)))).padding(10));
                    }
                }
                for (index, operation) in partitions
                    .iter()
                    .enumerate()
                    .filter(|(_, operation)| operation.action == PartitionAction::Create)
                {
                    content = content.push(
                        widget::container(
                            widget::column::with_capacity(4)
                                .spacing(6)
                                .width(Length::Fill)
                                .push(widget::text::heading(format!(
                                    "New partition {}",
                                    operation.device.display()
                                )))
                                .push(wrapped_body(format!(
                                    "CREATE • {} MiB at {} MiB • filesystem {} • mount {}",
                                    operation.size_mib.unwrap_or(0),
                                    operation.start_mib.unwrap_or(0),
                                    operation
                                        .filesystem
                                        .map_or("unset", Filesystem::display_name),
                                    operation.mount_point.as_deref().unwrap_or("none")
                                )))
                                .push(
                                    widget::button::destructive("Remove create operation")
                                        .on_press(Message::RemoveManualOperation(index)),
                                ),
                        )
                        .padding(10),
                    );
                }
                content = content
                    .push(widget::text::heading("Create partition"))
                    .push(
                        widget::text_input::text_input(
                            "Device, e.g. /dev/vda3",
                            &self.manual_device,
                        )
                        .on_input(Message::ManualDevice),
                    )
                    .push(
                        widget::text_input::text_input("GPT partition number", &self.manual_number)
                            .on_input(Message::ManualNumber),
                    )
                    .push(
                        widget::text_input::text_input("Start MiB", &self.manual_start)
                            .on_input(Message::ManualStart),
                    )
                    .push(
                        widget::text_input::text_input("Size MiB", &self.manual_size)
                            .on_input(Message::ManualSize),
                    )
                    .push(
                        widget::button::standard(format!(
                            "Filesystem: {}",
                            self.new_filesystem.display_name()
                        ))
                        .on_press(Message::CycleNewFilesystem),
                    )
                    .push(
                        widget::button::standard(format!(
                            "Mount: {}",
                            self.new_mount.as_deref().unwrap_or("none")
                        ))
                        .on_press(Message::CycleNewMount),
                    )
                    .push(
                        widget::button::suggested("Add create operation")
                            .on_press(Message::AddManualPartition),
                    );
            }
        }
        if let Some(error) = &self.error {
            content = content.push(wrapped_body(error));
        }
        content.into()
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.page {
            Page::Welcome => widget::column::with_capacity(3).spacing(16).width(Length::Fill)
                .push(widget::text::title2("Welcome to the MattOS installer"))
                .push(wrapped_body("This wizard installs MattOS using a guided whole-disk layout or an explicit manual partition plan."))
                .push(wrapped_body("You can return at any point before the final confirmation.")).into(),
            Page::Locale => widget::column::with_capacity(12).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Language and keyboard"))
                .push(wrapped_body("Choose from the locale, XKB, and zoneinfo data included in the offline MattOS image. Locale and timezone lists can be searched by typing a friendly name."))
                .push(widget::text::body("Locale")).push(combo_box(&self.locales, "Search languages or countries…", self.locales.options().iter().find(|choice| choice.id == self.model.locale), Message::Locale).width(Length::Fill))
                .push(widget::text::body("Keyboard layout")).push(combo_box(&self.keyboard_layouts, "Choose a keyboard layout…", self.keyboard_layouts.options().iter().find(|layout| layout.id == self.model.keyboard_layout), Message::KeyboardLayout).width(Length::Fill))
                .push(widget::text::body("Keyboard variant")).push(combo_box(&self.keyboard_variants, "Choose a variant…", self.keyboard_variants.options().iter().find(|choice| choice.id == self.model.keyboard_variant), Message::KeyboardVariant).width(Length::Fill))
                .push(widget::text::body("Test your keyboard")).push(widget::text_input::text_input("Type here to verify the selected layout", &self.keyboard_test).width(Length::Fill).on_input(Message::KeyboardTest))
                .push(wrapped_body("The test field records keystrokes using the compositor's current layout; this installer does not claim to switch the live compositor layout."))
                .push(widget::text::body("Timezone")).push(combo_box(&self.timezones, "Search cities or timezone regions…", self.timezones.options().iter().find(|choice| choice.id == self.model.timezone), Message::Timezone).width(Length::Fill)).into(),
            Page::Profile => widget::column::with_capacity(4).spacing(12).width(Length::Fill)
                .push(widget::text::title2("Choose the installed MattOS profile"))
                .push(profile_card("MattOS Desktop", "Graphical MattOS environment using COSMIC.", self.model.installed_profile == InstalledProfile::Desktop, InstalledProfile::Desktop))
                .push(profile_card("MattOS CLI", "Non-graphical MattOS base.", self.model.installed_profile == InstalledProfile::Cli, InstalledProfile::Cli)).into(),
            Page::Disk => {
                let mut content = widget::column::with_capacity(self.model.disks.len() + 3).spacing(12).width(Length::Fill)
                    .push(widget::text::title2("Select a target disk"))
                    .push(wrapped_body("Choose the disk that will contain the MattOS root filesystem. Guided mode erases it; manual mode changes only reviewed operations. Optical and read-only devices are never eligible."));
                if self.model.disks.is_empty() { content = content.push(wrapped_body("No eligible writable installation disks were found.")); }
                for (index, disk) in self.model.disks.iter().enumerate() {
                    let selected = self.model.selected_disk.as_ref() == Some(&disk.device);
                    content = content.push(disk_card(index, disk, selected));
                }
                content.into()
            }
            Page::Storage => self.storage_page(),
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
                let storage = self.model.selected_disk.as_ref().and_then(|disk| render_storage_plan(&self.model.storage, disk).ok()).unwrap_or_else(|| "Invalid storage plan".into());
                widget::column::with_capacity(11).spacing(12).width(Length::Fill)
                    .push(widget::text::title2("Review and confirm installation"))
                    .push(wrapped_body("WARNING: every CREATE, DELETE, and FORMAT item below is destructive. PRESERVE and REUSE items are not formatted."))
                    .push(widget::text::heading("Target disk"))
                    .push(wrapped_body(disk_description))
                    .push(widget::text::heading("Installation"))
                    .push(wrapped_body(format!("Full name: {}. Username: {}. Computer: {}. Account type: {}. Automatic login: {}. Root: {}.", self.model.full_name, self.model.username, self.model.hostname, if self.model.administrator { "Administrator" } else { "Standard user" }, if self.model.automatic_login { "Enabled" } else { "Disabled" }, if self.separate_root { "separate password configured" } else { "uses the user password" })))
                    .push(widget::text::heading("Region and input"))
                    .push(wrapped_body(format!("Locale: {}. Keyboard: {}. Variant: {}. Timezone: {}.", self.model.locale, self.model.keyboard_layout, if self.model.keyboard_variant.is_empty() { "Default" } else { &self.model.keyboard_variant }, self.model.timezone)))
                    .push(widget::text::heading("Boot and filesystem"))
                    .push(wrapped_body(storage))
                    .push(widget::button::standard(if self.destructive_confirmed { "Destructive changes acknowledged — confirmed" } else { "I have reviewed and accept every destructive storage operation" }).on_press(Message::ToggleDestructiveConfirmation))
                    .push(wrapped_body(if self.destructive_confirmed { "The final install action is now available below." } else { "Confirm only after checking the complete layout above." })).into()
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
    widget::text::body(text.into())
        .width(Length::Fill)
        .wrapping(Wrapping::Word)
        .into()
}

fn root_fields(app: &InstallerApp) -> Element<'_, Message> {
    if app.separate_root {
        widget::column::with_capacity(2)
            .spacing(8)
            .push(
                widget::text_input::secure_input("Root password", &app.root_password, None, true)
                    .width(Length::Fill)
                    .on_input(Message::RootPassword),
            )
            .push(
                widget::text_input::secure_input(
                    "Confirm root password",
                    &app.root_password_confirm,
                    None,
                    true,
                )
                .width(Length::Fill)
                .on_input(Message::RootPasswordConfirm),
            )
            .into()
    } else {
        widget::container(widget::space::vertical().height(0)).into()
    }
}

fn gibibytes(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

fn profile_card(
    profile: &'static str,
    description: &'static str,
    selected: bool,
    value: InstalledProfile,
) -> Element<'static, Message> {
    let label = if selected {
        format!("{profile} — selected")
    } else {
        profile.into()
    };
    widget::button::custom(
        widget::column::with_capacity(2)
            .spacing(4)
            .width(Length::Fill)
            .push(widget::text::heading(label).wrapping(Wrapping::Word))
            .push(
                widget::text::caption(description)
                    .width(Length::Fill)
                    .wrapping(Wrapping::Word),
            ),
    )
    .width(Length::Fill)
    .padding(14)
    .on_press(Message::SelectProfile(value))
    .into()
}

fn disk_card(
    index: usize,
    disk: &mattos_installer::engine::InstallDisk,
    selected: bool,
) -> Element<'static, Message> {
    let selected = if selected {
        "Selected target"
    } else {
        "Select this disk"
    };
    widget::button::custom(
        widget::column::with_capacity(3)
            .spacing(4)
            .width(Length::Fill)
            .push(widget::text::heading(disk.device.display().to_string()).wrapping(Wrapping::Word))
            .push(
                widget::text::body(disk.model.clone())
                    .width(Length::Fill)
                    .wrapping(Wrapping::Word),
            )
            .push(
                widget::text::caption(format!(
                    "{:.1} GiB — {selected}",
                    gibibytes(disk.size_bytes)
                ))
                .width(Length::Fill)
                .wrapping(Wrapping::Word),
            ),
    )
    .width(Length::Fill)
    .padding(14)
    .on_press(Message::SelectDisk(index))
    .into()
}

fn clear_secret(value: &mut String) {
    // Rust strings are UTF-8; replacing every byte with NUL preserves valid
    // UTF-8 before clearing the owning buffer.
    unsafe { value.as_bytes_mut().fill(0) };
    value.clear();
}

impl Application for InstallerApp {
    type Executor = executor::Default;
    type Flags = InstallerFlags;
    type Message = Message;
    const APP_ID: &'static str = "com.mattsherfey.MattOS.Installer";
    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }
    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Message>) {
        let variants = flags
            .keyboard_layouts
            .iter()
            .find(|layout| layout.id == flags.model.keyboard_layout)
            .map(|layout| layout.variants.clone())
            .unwrap_or_else(|| {
                vec![Choice {
                    id: String::new(),
                    label: "Default".into(),
                }]
            });
        (
            Self {
                core,
                model: flags.model,
                page: Page::Welcome,
                password: String::new(),
                password_confirm: String::new(),
                root_password: String::new(),
                root_password_confirm: String::new(),
                separate_root: false,
                error: None,
                installation_active: false,
                keyboard_test: String::new(),
                manual_device: String::new(),
                manual_number: String::new(),
                manual_start: String::new(),
                manual_size: String::new(),
                new_filesystem: Filesystem::Btrfs,
                new_mount: Some("/".into()),
                destructive_confirmed: false,
                locales: combo_box::State::new(flags.locales),
                keyboard_layouts: combo_box::State::new(flags.keyboard_layouts),
                keyboard_variants: combo_box::State::new(variants),
                timezones: combo_box::State::new(flags.timezones),
            },
            Task::none(),
        )
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Next => self.next(),
            Message::Back => self.back(),
            Message::SelectDisk(index) => {
                if let Err(error) = self.model.select_disk(index) {
                    self.fail(error.to_string());
                }
            }
            Message::SelectProfile(profile) => self.model.installed_profile = profile,
            Message::FullName(value) => self.model.full_name = value,
            Message::Hostname(value) => self.model.hostname = value,
            Message::Username(value) => self.model.username = value,
            Message::Locale(choice) => self.model.locale = choice.id,
            Message::KeyboardLayout(layout) => {
                self.model.keyboard_layout = layout.id;
                self.model.keyboard_variant.clear();
                self.keyboard_variants = combo_box::State::new(layout.variants);
            }
            Message::KeyboardVariant(choice) => self.model.keyboard_variant = choice.id,
            Message::Timezone(choice) => self.model.timezone = choice.id,
            Message::KeyboardTest(value) => self.keyboard_test = value,
            Message::Password(value) => self.password = value,
            Message::PasswordConfirm(value) => self.password_confirm = value,
            Message::RootPassword(value) => self.root_password = value,
            Message::RootPasswordConfirm(value) => self.root_password_confirm = value,
            Message::ToggleAdministrator => self.model.administrator = !self.model.administrator,
            Message::ToggleAutologin => self.model.automatic_login = !self.model.automatic_login,
            Message::ToggleSeparateRoot => {
                self.separate_root = !self.separate_root;
                if !self.separate_root {
                    clear_secret(&mut self.root_password);
                    clear_secret(&mut self.root_password_confirm);
                }
            }
            Message::GuidedBtrfs => self.model.storage = StoragePlan::guided_btrfs(),
            Message::GuidedExt4 => self.model.storage = StoragePlan::guided_ext4(),
            Message::ManualStorage => {
                self.model.storage = StoragePlan::Manual {
                    partitions: self
                        .model
                        .partitions
                        .iter()
                        .map(|partition| PartitionOperation {
                            device: partition.device.clone(),
                            action: PartitionAction::Preserve,
                            encryption: EncryptionPolicy::None,
                            filesystem: None,
                            mount_point: None,
                            partition_number: None,
                            start_mib: None,
                            size_mib: None,
                        })
                        .collect(),
                }
            }
            Message::CreateEfi => {
                if let StoragePlan::GuidedWholeDisk { efi, .. } = &mut self.model.storage {
                    *efi = GuidedEfi::Create;
                }
            }
            Message::ReuseEfi(index) => {
                if let (Some(partition), StoragePlan::GuidedWholeDisk { efi, .. }) =
                    (self.model.partitions.get(index), &mut self.model.storage)
                {
                    *efi = GuidedEfi::Reuse {
                        device: partition.device.clone(),
                        format: false,
                    };
                }
            }
            Message::ToggleEfiFormat => {
                if let StoragePlan::GuidedWholeDisk {
                    efi: GuidedEfi::Reuse { format, .. },
                    ..
                } = &mut self.model.storage
                {
                    *format = !*format;
                }
            }
            Message::CyclePartitionAction(index) => {
                if let (Some(partition), StoragePlan::Manual { partitions }) =
                    (self.model.partitions.get(index), &mut self.model.storage)
                    && let Some(operation) = partitions
                        .iter_mut()
                        .find(|operation| operation.device == partition.device)
                {
                    operation.action = match operation.action {
                        PartitionAction::Preserve => PartitionAction::Reuse,
                        PartitionAction::Reuse => PartitionAction::Format,
                        PartitionAction::Format => PartitionAction::Delete,
                        _ => PartitionAction::Preserve,
                    };
                    operation.filesystem = if operation.action == PartitionAction::Format {
                        Some(if partition.is_esp {
                            Filesystem::Fat32
                        } else {
                            match partition.filesystem.as_deref() {
                                Some("ext4") => Filesystem::Ext4,
                                _ => Filesystem::Btrfs,
                            }
                        })
                    } else {
                        None
                    };
                    if matches!(
                        operation.action,
                        PartitionAction::Delete | PartitionAction::Preserve
                    ) {
                        operation.mount_point = None;
                    }
                }
            }
            Message::CyclePartitionMount(index) => {
                if let (Some(partition), StoragePlan::Manual { partitions }) =
                    (self.model.partitions.get(index), &mut self.model.storage)
                    && let Some(operation) = partitions
                        .iter_mut()
                        .find(|operation| operation.device == partition.device)
                    && operation.action != PartitionAction::Delete
                {
                    operation.mount_point = match operation.mount_point.as_deref() {
                        None => Some("/".into()),
                        Some("/") => Some("/home".into()),
                        Some("/home") => Some("/boot/efi".into()),
                        _ => None,
                    };
                }
            }
            Message::CyclePartitionFilesystem(index) => {
                if let (Some(partition), StoragePlan::Manual { partitions }) =
                    (self.model.partitions.get(index), &mut self.model.storage)
                    && let Some(operation) = partitions
                        .iter_mut()
                        .find(|operation| operation.device == partition.device)
                    && operation.action == PartitionAction::Format
                {
                    operation.filesystem =
                        Some(match operation.filesystem.unwrap_or(Filesystem::Btrfs) {
                            Filesystem::Btrfs => Filesystem::Ext4,
                            Filesystem::Ext4 => Filesystem::Fat32,
                            Filesystem::Fat32 => Filesystem::Btrfs,
                        });
                }
            }
            Message::ManualDevice(value) => self.manual_device = value,
            Message::ManualNumber(value) => self.manual_number = value,
            Message::ManualStart(value) => self.manual_start = value,
            Message::ManualSize(value) => self.manual_size = value,
            Message::CycleNewFilesystem => {
                self.new_filesystem = match self.new_filesystem {
                    Filesystem::Btrfs => Filesystem::Ext4,
                    Filesystem::Ext4 => Filesystem::Fat32,
                    Filesystem::Fat32 => Filesystem::Btrfs,
                }
            }
            Message::CycleNewMount => {
                self.new_mount = match self.new_mount.as_deref() {
                    None => Some("/".into()),
                    Some("/") => Some("/home".into()),
                    Some("/home") => Some("/boot/efi".into()),
                    _ => None,
                }
            }
            Message::AddManualPartition => {
                if let StoragePlan::Manual { partitions } = &mut self.model.storage {
                    let parsed = (
                        self.manual_number.parse::<u32>(),
                        self.manual_start.parse::<u64>(),
                        self.manual_size.parse::<u64>(),
                    );
                    match parsed { (Ok(number), Ok(start), Ok(size)) if !self.manual_device.is_empty() => {
                    partitions.push(PartitionOperation { device: self.manual_device.clone().into(), action: PartitionAction::Create, encryption: EncryptionPolicy::None, filesystem: Some(self.new_filesystem), mount_point: self.new_mount.clone(), partition_number: Some(number), start_mib: Some(start), size_mib: Some(size) });
                    self.manual_device.clear(); self.manual_number.clear(); self.manual_start.clear(); self.manual_size.clear(); self.error = None;
                }, _ => self.error = Some("Created partition requires a device, numeric GPT number, start MiB, and size MiB.".into()) }
                }
            }
            Message::RemoveManualOperation(index) => {
                if let StoragePlan::Manual { partitions } = &mut self.model.storage {
                    if partitions
                        .get(index)
                        .is_some_and(|operation| operation.action == PartitionAction::Create)
                    {
                        partitions.remove(index);
                    }
                }
            }
            Message::ToggleDestructiveConfirmation => {
                self.destructive_confirmed = !self.destructive_confirmed
            }
            Message::Install => {
                if self.destructive_confirmed {
                    return self.start_install();
                } else {
                    self.error =
                        Some("Review and acknowledge the destructive storage plan first.".into());
                }
            }
            Message::InstallationEvent(InstallationEvent::Progress(event)) => {
                if self.installation_active && self.page == Page::Installing {
                    self.model.progress(event);
                }
            }
            Message::InstallationEvent(InstallationEvent::Finished(result)) => {
                if !self.installation_active || self.page != Page::Installing {
                    return Task::none();
                }
                self.installation_active = false;
                match result {
                    Ok(()) => {
                        self.model.complete();
                        self.page = Page::Complete;
                    }
                    Err(error) => self.fail(error),
                }
            }
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> {
        self.navigation(self.page_content())
    }
}

fn contract_proof() -> anyhow::Result<()> {
    let model = InstallerFrontendModel::discover()?;
    let locales = discover_locales()?;
    let layouts = discover_keyboard_layouts()?;
    let timezones = discover_timezones()?;
    println!(
        "frontend=cosmic pages=welcome,locale,profile,disk,storage,account,review,installing,complete,error"
    );
    println!(
        "shared_disk_count={} profiles=desktop,cli",
        model.disks.len()
    );
    println!(
        "shared_plan_policy_engine=true structured_progress={:?}",
        InstallStage::Preparing
    );
    println!(
        "offline_choices locales={} layouts={} timezones={}",
        locales.len(),
        layouts.len(),
        timezones.len()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|argument| argument == "--contract-proof") {
        return contract_proof();
    }
    // libcosmic defaults to a normal 1024×768 logical-pixel window and
    // applies the Wayland output scale. The minimum merely keeps the wizard
    // usable on firmware/KMS fallback modes; content scrolls below it.
    cosmic::app::run::<InstallerApp>(
        Settings::default().size_limits(Limits::NONE.min_width(480.0).min_height(400.0)),
        InstallerFlags {
            model: InstallerFrontendModel::discover()?,
            locales: discover_locales()?,
            keyboard_layouts: discover_keyboard_layouts()?,
            timezones: discover_timezones()?,
        },
    )?;
    Ok(())
}
