//! MattOS-owned installation subsystem.
//!
//! Frontends collect a versioned plan. [`policy`] defines what that plan means
//! for MattOS, while [`engine`] owns reusable destructive-operation mechanics.

pub mod engine;
#[path = "gui/model.rs"]
pub mod gui_model;
pub mod policy;

pub use policy::{
    EncryptionPolicy, Filesystem, GuidedEfi, InstallPlan, InstallProgress, InstallStage,
    InstalledProfile, PLAN_VERSION, PartitionAction, PartitionOperation, RootCredentialPolicy,
    RootFilesystem, StoragePlan, execute, execute_with_progress, render_plan, render_storage_plan,
};
