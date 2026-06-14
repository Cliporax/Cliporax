//! Permission system module

pub mod checker;
pub mod definition;

pub use checker::{PermissionChecker, PermissionEvaluation, PermissionResult};
pub use definition::{builtin_permissions, Permission, PermissionCategory, RiskLevel};
