//! MattOS-owned installation subsystem.
//!
//! Frontends collect a versioned plan. [`policy`] defines what that plan means
//! for MattOS, while [`engine`] owns reusable destructive-operation mechanics.

pub mod discovery;
pub mod engine;
#[path = "gui/model.rs"]
pub mod gui_model;
pub mod policy;

pub use discovery::{
    Choice, KeyboardLayout, discover_keyboard_layouts, discover_locales, discover_timezones,
};

pub use policy::{
    EncryptionPolicy, Filesystem, GuidedEfi, InstallPlan, InstallProgress, InstallStage,
    InstalledProfile, OptionalPackage, OptionalPackageBackend, PLAN_VERSION, PartitionAction,
    PartitionOperation, RootCredentialPolicy, RootFilesystem, StoragePlan, execute,
    execute_with_progress, optional_package, optional_package_catalog, optional_package_defaults,
    render_plan, render_storage_plan,
};
