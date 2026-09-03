//! Toolkit-neutral state shared by graphical installer frontends.

use crate::{
    InstallPlan, InstallProgress, InstalledProfile, RootCredentialPolicy, StoragePlan, engine,
    optional_package, optional_package_defaults, render_plan,
};
use anyhow::{Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontendState {
    Planning,
    Validated,
    Installing(InstallProgress),
    Complete,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct InstallerFrontendModel {
    pub disks: Vec<engine::InstallDisk>,
    pub partitions: Vec<engine::InstallPartition>,
    pub selected_disk: Option<PathBuf>,
    pub storage: StoragePlan,
    pub installed_profile: InstalledProfile,
    pub optional_packages: Vec<String>,
    pub hostname: String,
    pub full_name: String,
    pub username: String,
    pub administrator: bool,
    pub automatic_login: bool,
    pub locale: String,
    pub keyboard_layout: String,
    pub keyboard_variant: String,
    pub timezone: String,
    pub state: FrontendState,
}

impl InstallerFrontendModel {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            disks: engine::discover_install_disks()?,
            partitions: Vec::new(),
            selected_disk: None,
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Desktop,
            optional_packages: optional_package_defaults(InstalledProfile::Desktop),
            hostname: "mattos".into(),
            full_name: "MattOS User".into(),
            username: "mattos".into(),
            administrator: true,
            automatic_login: false,
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            state: FrontendState::Planning,
        })
    }

    pub fn select_disk(&mut self, index: usize) -> Result<()> {
        let disk = self
            .disks
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("invalid installer disk selection {index}"))?;
        self.selected_disk = Some(disk.device.clone());
        self.partitions.clear();
        for candidate in &self.disks {
            self.partitions
                .extend(engine::discover_partitions(&candidate.device)?);
        }
        self.partitions
            .sort_by(|left, right| left.device.cmp(&right.device));
        self.storage = StoragePlan::guided_btrfs();
        self.state = FrontendState::Planning;
        Ok(())
    }

    pub fn select_profile(&mut self, profile: InstalledProfile) {
        self.installed_profile = profile;
        self.optional_packages = optional_package_defaults(profile);
        self.state = FrontendState::Planning;
    }

    pub fn toggle_optional_package(&mut self, id: &str) -> Result<()> {
        if optional_package(id).is_none() {
            bail!("unknown optional package {id}");
        }
        if let Some(index) = self.optional_packages.iter().position(|selected| selected == id) {
            self.optional_packages.remove(index);
        } else {
            self.optional_packages.push(id.to_owned());
            self.optional_packages.sort();
        }
        self.state = FrontendState::Planning;
        Ok(())
    }

    pub fn plan(
        &self,
        password_hash: Option<String>,
        root_credential: RootCredentialPolicy,
    ) -> Result<InstallPlan> {
        let Some(target_disk) = self.selected_disk.clone() else {
            bail!("select an explicit target disk before validating the installation plan");
        };
        let plan = InstallPlan {
            version: crate::PLAN_VERSION,
            target_disk,
            storage: self.storage.clone(),
            installed_profile: self.installed_profile,
            optional_packages: self.optional_packages.clone(),
            hostname: self.hostname.clone(),
            full_name: self.full_name.clone(),
            username: self.username.clone(),
            password_hash,
            administrator: self.administrator,
            automatic_login: self.automatic_login,
            root_credential,
            locale: self.locale.clone(),
            keyboard_layout: self.keyboard_layout.clone(),
            keyboard_variant: self.keyboard_variant.clone(),
            timezone: self.timezone.clone(),
            test_autologin: false,
        };
        plan.validate_policy()?;
        Ok(plan)
    }

    pub fn summary(&self) -> Result<String> {
        render_plan(&self.plan(None, RootCredentialPolicy::SameAsUser)?)
    }

    pub fn mark_validated(&mut self) {
        self.state = FrontendState::Validated;
    }

    pub fn progress(&mut self, event: InstallProgress) {
        self.state = FrontendState::Installing(event);
    }

    pub fn complete(&mut self) {
        self.state = FrontendState::Complete;
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = FrontendState::Failed(error.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_graphical_model_requires_an_explicit_disk() {
        let model = InstallerFrontendModel {
            disks: Vec::new(),
            partitions: Vec::new(),
            selected_disk: None,
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Cli,
            optional_packages: Vec::new(),
            hostname: "mattos".into(),
            full_name: "Test User".into(),
            administrator: true,
            automatic_login: false,
            username: "tester".into(),
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            state: FrontendState::Planning,
        };
        assert!(
            model
                .plan(None, RootCredentialPolicy::SameAsUser)
                .unwrap_err()
                .to_string()
                .contains("explicit target disk")
        );
    }

    #[test]
    fn shared_graphical_model_exposes_progress_and_errors() {
        let mut model = InstallerFrontendModel {
            disks: Vec::new(),
            partitions: Vec::new(),
            selected_disk: None,
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Desktop,
            optional_packages: optional_package_defaults(InstalledProfile::Desktop),
            hostname: "mattos".into(),
            full_name: "Test User".into(),
            administrator: true,
            automatic_login: false,
            username: "tester".into(),
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            state: FrontendState::Planning,
        };
        model.mark_validated();
        assert_eq!(model.state, FrontendState::Validated);
        model.progress(InstallProgress {
            stage: crate::InstallStage::Partitioning,
            completed_stages: 1,
            total_stages: 10,
            detail: "partitioning".into(),
        });
        assert!(
            matches!(model.state, FrontendState::Installing(InstallProgress { ref detail, .. }) if detail == "partitioning")
        );
        model.fail("disk failed");
        assert_eq!(model.state, FrontendState::Failed("disk failed".into()));
    }

    #[test]
    fn shared_graphical_model_applies_each_intermediate_progress_event() {
        let mut model = InstallerFrontendModel {
            disks: Vec::new(),
            partitions: Vec::new(),
            selected_disk: None,
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Desktop,
            optional_packages: optional_package_defaults(InstalledProfile::Desktop),
            hostname: "mattos".into(),
            full_name: "Test User".into(),
            username: "tester".into(),
            administrator: true,
            automatic_login: false,
            state: FrontendState::Validated,
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
        };
        for (stage, completed, detail) in [
            (crate::InstallStage::Preparing, 0, "validating"),
            (crate::InstallStage::Partitioning, 1, "partitioning"),
            (crate::InstallStage::Formatting, 2, "formatting"),
        ] {
            model.progress(InstallProgress {
                stage,
                completed_stages: completed,
                total_stages: 10,
                detail: detail.into(),
            });
            assert!(
                matches!(&model.state, FrontendState::Installing(event) if event.detail == detail)
            );
        }
        model.complete();
        assert_eq!(model.state, FrontendState::Complete);
    }

    #[test]
    fn graphical_model_keeps_credentials_out_of_rendered_plan() {
        let model = InstallerFrontendModel {
            disks: Vec::new(),
            partitions: Vec::new(),
            selected_disk: Some("/dev/vda".into()),
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Desktop,
            optional_packages: optional_package_defaults(InstalledProfile::Desktop),
            hostname: "mattos".into(),
            full_name: "Test User".into(),
            administrator: false,
            automatic_login: false,
            username: "tester".into(),
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            state: FrontendState::Planning,
        };
        let plan = model
            .plan(
                Some("$6$only-a-hash".into()),
                RootCredentialPolicy::SameAsUser,
            )
            .unwrap();
        let rendered = render_plan(&plan).unwrap();
        assert!(!rendered.contains("only-a-hash"));
        assert!(!rendered.contains("password"));
    }

    #[test]
    fn graphical_model_defaults_and_persists_explicit_optional_selection() {
        let mut model = InstallerFrontendModel {
            disks: Vec::new(),
            partitions: Vec::new(),
            selected_disk: Some("/dev/vda".into()),
            storage: StoragePlan::guided_btrfs(),
            installed_profile: InstalledProfile::Desktop,
            optional_packages: optional_package_defaults(InstalledProfile::Desktop),
            hostname: "mattos".into(),
            full_name: "Test User".into(),
            username: "tester".into(),
            administrator: false,
            automatic_login: false,
            locale: "en_US.UTF-8".into(),
            keyboard_layout: "us".into(),
            keyboard_variant: String::new(),
            timezone: "Etc/UTC".into(),
            state: FrontendState::Planning,
        };
        assert_eq!(model.optional_packages, ["firefox"]);
        model.toggle_optional_package("firefox").unwrap();
        assert!(model.optional_packages.is_empty());
        assert!(model
            .toggle_optional_package("not-in-catalog")
            .unwrap_err()
            .to_string()
            .contains("unknown optional package"));
        assert!(model
            .plan(None, RootCredentialPolicy::SameAsUser)
            .unwrap()
            .optional_packages
            .is_empty());

        model.select_profile(InstalledProfile::Cli);
        assert!(model.optional_packages.is_empty());
        model.select_profile(InstalledProfile::Desktop);
        assert_eq!(model.optional_packages, ["firefox"]);
    }
}
