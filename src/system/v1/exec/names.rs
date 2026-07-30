//! Run name generation using `petname`.

use petname::Petnames;
use rand::RngExt;

/// Generate a random run name.
///
/// Returns a name in the format `adjective-noun-xxxxxx` (e.g.,
/// `happy-elephant-a1b2c3`).
///
/// The hex suffix ensures uniqueness even with duplicate petnames.
pub fn generate_run_name() -> String {
    // Generate 6 random hex digits for the suffix.
    let mut rng = rand::rng();
    let hex_suffix = format!("{:06x}", rng.random::<u32>() & 0xFFFFFF);

    let names = Petnames::large();
    format!(
        "{}-{}",
        names
            .namer(2, "-")
            .iter(&mut rng)
            .next()
            .expect("failed to generate petname"),
        hex_suffix
    )
}
