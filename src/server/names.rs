//! Workflow name generation using petname.

use petname::Generator;
use petname::Petnames;
use rand::Rng;

/// Generate a random workflow name.
///
/// Returns a name in the format `adjective-noun-xxxxxx` (e.g., `happy-elephant-a1b2c3`).
/// The hex suffix ensures uniqueness even with duplicate petnames.
pub fn generate_workflow_name() -> String {
    let petname = Petnames::default()
        .generate_one(2, "-")
        .expect("failed to generate petname");

    // Generate 6 random hex digits.
    let mut rng = rand::rng();
    let hex_suffix = format!("{:06x}", rng.random::<u32>() & 0xFFFFFF);

    format!("{}-{}", petname, hex_suffix)
}
