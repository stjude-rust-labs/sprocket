//! Module for unit representations.

use std::str::FromStr;

/// Represents a storage unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StorageUnit {
    /// The unit is in bytes.
    #[default]
    Bytes,
    /// The unit is in kilobytes (10^3 bytes).
    Kilobytes,
    /// The unit is in megabytes (10^6 bytes).
    Megabytes,
    /// The unit is in gigabytes (10^9 bytes).
    Gigabytes,
    /// The unit is in terabytes (10^12 bytes).
    Terabytes,
    /// The unit is in kibibytes (2^10 bytes).
    Kibibytes,
    /// The unit is in mebibytes (2^20 bytes).
    Mebibytes,
    /// The unit is in gibibytes (2^30 bytes).
    Gibibytes,
    /// The unit is in tebibytes (2^40 bytes).
    Tebibytes,
}

impl StorageUnit {
    /// Converts the given number of bytes into a float representing the number
    /// of units.
    pub fn units(&self, bytes: u64) -> f64 {
        let bytes = bytes as f64;
        match self {
            Self::Bytes => bytes,
            Self::Kilobytes => bytes / 1000.0,
            Self::Megabytes => bytes / 1000000.0,
            Self::Gigabytes => bytes / 1000000000.0,
            Self::Terabytes => bytes / 1000000000000.0,
            Self::Kibibytes => bytes / 1024.0,
            Self::Mebibytes => bytes / 1048576.0,
            Self::Gibibytes => bytes / 1073741824.0,
            Self::Tebibytes => bytes / 1099511627776.0,
        }
    }

    /// Converts the given number of bytes into the corresponding number of
    /// bytes based on the unit.
    pub fn bytes(&self, bytes: u64) -> Option<u64> {
        match self {
            Self::Bytes => Some(bytes),
            Self::Kilobytes => bytes.checked_mul(1000),
            Self::Megabytes => bytes.checked_mul(1000000),
            Self::Gigabytes => bytes.checked_mul(1000000000),
            Self::Terabytes => bytes.checked_mul(1000000000000),
            Self::Kibibytes => bytes.checked_mul(1024),
            Self::Mebibytes => bytes.checked_mul(1048576),
            Self::Gibibytes => bytes.checked_mul(1073741824),
            Self::Tebibytes => bytes.checked_mul(1099511627776),
        }
    }
}

impl FromStr for StorageUnit {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "B" => Ok(Self::Bytes),
            "KB" | "K" => Ok(Self::Kilobytes),
            "MB" | "M" => Ok(Self::Megabytes),
            "GB" | "G" => Ok(Self::Gigabytes),
            "TB" | "T" => Ok(Self::Terabytes),
            "KiB" | "Ki" => Ok(Self::Kibibytes),
            "MiB" | "Mi" => Ok(Self::Mebibytes),
            "GiB" | "Gi" => Ok(Self::Gibibytes),
            "TiB" | "Ti" => Ok(Self::Tebibytes),
            _ => Err(()),
        }
    }
}

/// Converts a unit string (e.g. `2 GiB`) to bytes.
///
/// The string is expected to contain a single integer followed by the unit.
///
/// Returns `None` if the string is not a valid unit string or if the resulting
/// byte count exceeds an unsigned 64-bit integer.
pub fn convert_unit_string(s: &str) -> Option<u64> {
    // No space, so try splitting on first alpha
    let (n, unit) = match s.chars().position(|c| c.is_ascii_alphabetic()) {
        Some(index) => {
            let (n, unit) = s.split_at(index);
            (
                n.trim().parse::<u64>().ok()?,
                unit.trim().parse::<StorageUnit>().ok()?,
            )
        }
        None => return None,
    };

    unit.bytes(n)
}
