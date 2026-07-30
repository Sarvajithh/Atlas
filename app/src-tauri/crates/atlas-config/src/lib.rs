//! atlas-config
//!
//! Centralized configuration (§23, §33.12). Every value that may plausibly
//! change — model names, paths, ports, chunk sizes, prompt templates, UI
//! constants, feature flags — is read through the `SettingsProvider`
//! interface defined here, never embedded in business logic (Governing
//! Principle, §46.1).
//!
//! No crate reads the `settings` table directly (§33.12); all access goes
//! through this interface. [`LayeredSettingsProvider`] provides the
//! hierarchical (default < user < workspace < runtime) resolution policy;
//! validation of every entry happens in [`validation`] before it is
//! accepted into a layer.

pub mod hierarchy;
pub mod provider;
pub mod validation;

pub use hierarchy::{ConfigLayer, LayeredSettingsProvider};
pub use provider::SettingsProvider;
