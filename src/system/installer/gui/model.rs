//! Toolkit-neutral state shared by graphical installer frontends.

use crate::{InstallPlan, InstallProgress, InstalledProfile, engine, render_plan};
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
    pub selected_disk: Option<PathBuf>,
    pub installed_profile: InstalledProfile,
    pub hostname: String,
    pub username: String,
    pub state: FrontendState,
}

impl InstallerFrontendModel {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            disks: engine::discover_install_disks()?,
            selected_disk: None,
            installed_profile: InstalledProfile::Desktop,
            hostname: "mattos".into(),
            username: "mattos".into(),
            state: FrontendState::Planning,
        })
    }

    pub fn select_disk(&mut self, index: usize) -> Result<()> {
        let disk = self
            .disks
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("invalid installer disk selection {index}"))?;
        self.selected_disk = Some(disk.device.clone());
        self.state = FrontendState::Planning;
        Ok(())
    }

    pub fn plan(&self, password_hash: Option<String>) -> Result<InstallPlan> {
        let Some(target_disk) = self.selected_disk.clone() else {
            bail!("select an explicit target disk before validating the installation plan");
        };
        let plan = InstallPlan {
            version: crate::PLAN_VERSION,
            target_disk,
            installed_profile: self.installed_profile,
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            password_hash,
            test_autologin: false,
        };
        plan.validate_policy()?;
        Ok(plan)
    }

    pub fn summary(&self) -> Result<String> {
        render_plan(&self.plan(None)?)
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
            selected_disk: None,
            installed_profile: InstalledProfile::Cli,
            hostname: "mattos".into(),
            username: "tester".into(),
            state: FrontendState::Planning,
        };
        assert!(model.plan(None).unwrap_err().to_string().contains("explicit target disk"));
    }

    #[test]
    fn shared_graphical_model_exposes_progress_and_errors() {
        let mut model = InstallerFrontendModel {
            disks: Vec::new(),
            selected_disk: None,
            installed_profile: InstalledProfile::Desktop,
            hostname: "mattos".into(),
            username: "tester".into(),
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
        assert!(matches!(model.state, FrontendState::Installing(InstallProgress { ref detail, .. }) if detail == "partitioning"));
        model.fail("disk failed");
        assert_eq!(model.state, FrontendState::Failed("disk failed".into()));
    }

    #[test]
    fn graphical_model_keeps_credentials_out_of_rendered_plan() {
        let model = InstallerFrontendModel {
            disks: Vec::new(),
            selected_disk: Some("/dev/vda".into()),
            installed_profile: InstalledProfile::Desktop,
            hostname: "mattos".into(),
            username: "tester".into(),
            state: FrontendState::Planning,
        };
        let plan = model.plan(Some("$6$only-a-hash".into())).unwrap();
        let rendered = render_plan(&plan).unwrap();
        assert!(!rendered.contains("only-a-hash"));
        assert!(!rendered.contains("password"));
    }
}
