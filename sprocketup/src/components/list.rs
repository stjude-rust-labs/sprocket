//! List components in an installation.

use clap::builder::styling::Style;
use clap_cargo::style::CONTEXT;

use crate::components::Components;
use crate::manifest::Manifest;

/// List the components in the current installation.
pub fn list_components(installed_only: bool, quiet: bool) -> anyhow::Result<()> {
    let installed_components = Components::load(None)?;
    let manifest = Manifest::load()?;

    let bold = Style::new().bold();
    for component in manifest.components() {
        let installed = installed_components
            .list()
            .iter()
            .any(|installed_component| installed_component.name == component);
        if installed && !installed_only && !quiet {
            println!("{bold}{component}{bold:#} {CONTEXT}(installed){CONTEXT:#}",);
        } else if installed || !installed_only {
            println!("{component}");
        }
    }

    Ok(())
}
