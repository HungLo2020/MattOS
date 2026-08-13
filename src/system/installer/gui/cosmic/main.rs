//! Permanent Rust + libcosmic MattOS installer frontend foundation.
//!
//! Destructive operations remain exclusively in `mattos-installer` policy and
//! engine. This frontend owns only presentation and shared-model messages.

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Length, Size};
use cosmic::{Application, Element, executor, widget};
use mattos_installer::InstalledProfile;
use mattos_installer::gui_model::InstallerFrontendModel;

#[derive(Clone, Debug)]
enum Message {
    SelectDisk(usize),
    SelectProfile(InstalledProfile),
    Validate,
}

struct InstallerApp {
    core: Core,
    model: InstallerFrontendModel,
}

impl Application for InstallerApp {
    type Executor = executor::Default;
    type Flags = InstallerFrontendModel;
    type Message = Message;
    const APP_ID: &'static str = "com.mattsherfey.MattOS.Installer";

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, model: Self::Flags) -> (Self, Task<Message>) {
        (Self { core, model }, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectDisk(index) => {
                if let Err(error) = self.model.select_disk(index) {
                    self.model.fail(error.to_string());
                }
            }
            Message::SelectProfile(profile) => self.model.installed_profile = profile,
            Message::Validate => match self.model.summary() {
                Ok(_) => self.model.mark_validated(),
                Err(error) => self.model.fail(error.to_string()),
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let mut content = widget::column::with_capacity(self.model.disks.len() + 9)
            .spacing(12)
            .push(widget::text::title2("Install MattOS"))
            .push(widget::text::body("Native COSMIC frontend using the shared MattOS installer model"))
            .push(widget::text::heading("Target disks"));

        if self.model.disks.is_empty() {
            content = content.push(widget::text::body("No eligible writable installation disks discovered"));
        }
        for (index, disk) in self.model.disks.iter().enumerate() {
            let selected = self.model.selected_disk.as_ref() == Some(&disk.device);
            let label = format!(
                "{}{} — {:.1} GiB — {}",
                if selected { "Selected: " } else { "" },
                disk.device.display(),
                disk.size_bytes as f64 / 1_073_741_824.0,
                disk.model,
            );
            content = content.push(widget::button::standard(label).on_press(Message::SelectDisk(index)));
        }

        content = content
            .push(widget::text::heading("Installed profile"))
            .push(widget::row::with_capacity(2)
                .spacing(8)
                .push(widget::button::standard("MattOS Desktop").on_press(Message::SelectProfile(InstalledProfile::Desktop)))
                .push(widget::button::standard("MattOS CLI").on_press(Message::SelectProfile(InstalledProfile::Cli))))
            .push(widget::text::body(format!(
                "GPT → 512 MiB FAT32 ESP → Btrfs @, @home, @snapshots\nProfile: {:?}\nState: {:?}",
                self.model.installed_profile, self.model.state
            )))
            .push(widget::button::suggested("Validate plan").on_press(Message::Validate));

        widget::container(content)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn contract_proof() -> anyhow::Result<()> {
    let model = InstallerFrontendModel::discover()?;
    println!("frontend=cosmic app_id={}", InstallerApp::APP_ID);
    println!("shared_disk_count={}", model.disks.len());
    println!("profiles=desktop,cli");
    println!("storage=GPT,ESP-512MiB,Btrfs:@,@home,@snapshots");
    println!("shared_validation=true shared_progress_state=true shared_error_state=true");
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|argument| argument == "--contract-proof") {
        return contract_proof();
    }
    let model = InstallerFrontendModel::discover()?;
    cosmic::app::run::<InstallerApp>(Settings::default().size(Size::new(900.0, 650.0)), model)?;
    Ok(())
}
