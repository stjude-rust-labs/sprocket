//! Component subcommands.

mod add;
mod list;
mod remove;

use std::path::Path;
use std::path::PathBuf;

pub(crate) use add::ComponentInstallOptions;
pub(crate) use add::add_components;
pub(crate) use add::link_components_to_sprocket;
use anyhow::bail;
pub(crate) use list::list_components;
pub(crate) use remove::remove_components;
use serde::Deserialize;
use serde::Serialize;

use crate::dirs::current_profile_dir;

/// Serializable companion type for [`Components`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SerializedComponents {
    /// The installed components.
    #[serde(default)]
    components: Vec<Component>,
}

impl From<Components> for SerializedComponents {
    fn from(value: Components) -> Self {
        Self {
            components: value.components,
        }
    }
}

/// A list of installed components.
#[derive(Clone, Debug, Serialize)]
#[serde(into = "SerializedComponents")]
pub(crate) struct Components {
    /// The base of the current profile.
    profile_base: PathBuf,
    /// The installed components.
    components: Vec<Component>,
}

/// Get the path to the components file.
fn components_manifest(profile_base: &Path) -> anyhow::Result<PathBuf> {
    let manifest = profile_base.join("components");
    if !manifest.exists() {
        bail!(
            "Expected component manifest at {} (corrupted installation?)",
            manifest.display()
        );
    }

    Ok(manifest)
}

impl Components {
    /// Load the installed components in the current profile.
    pub(crate) fn load(profile_base: Option<PathBuf>) -> anyhow::Result<Self> {
        let profile_base = match profile_base {
            Some(profile_base) => profile_base,
            None => current_profile_dir()?,
        };

        let components_manifest = std::fs::read_to_string(components_manifest(&profile_base)?)?;
        let serialized_components = toml::from_str::<SerializedComponents>(&components_manifest)?;

        Ok(Self {
            profile_base,
            components: serialized_components.components,
        })
    }

    /// Get all installed components.
    pub(crate) fn list(&self) -> &[Component] {
        &self.components
    }

    /// Start a transaction on the component list.
    pub(crate) fn tx(&mut self) -> ComponentsTx<'_> {
        ComponentsTx {
            components: self,
            installed: Vec::new(),
            removed: Vec::new(),
        }
    }
}

// TODO: Make a single, global `Components` instance
/// A transaction on a set of components.
pub(crate) struct ComponentsTx<'a> {
    /// Reference to the components to mutate.
    components: &'a mut Components,
    /// The components installed during this transaction.
    installed: Vec<String>,
    /// The components removed during this transaction.
    removed: Vec<String>,
}

impl ComponentsTx<'_> {
    /// A component was installed.
    pub(crate) fn installed(&mut self, component: impl Into<String>) {
        self.installed.push(component.into());
    }

    /// A component was removed.
    pub(crate) fn removed(&mut self, component: impl Into<String>) {
        self.removed.push(component.into());
    }

    /// Commit this transaction to disk.
    pub(crate) fn commit(self) -> anyhow::Result<()> {
        self.components.components.extend(
            self.installed
                .into_iter()
                .map(|component| Component { name: component }),
        );
        self.components
            .components
            .retain(|component| !self.removed.contains(&component.name));

        let new_manifest = toml::to_string_pretty(self.components)?;
        std::fs::write(
            components_manifest(&self.components.profile_base)?,
            new_manifest,
        )?;

        Ok(())
    }
}

/// An installed component.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Component {
    /// The binary name of the component.
    pub(crate) name: String,
}
