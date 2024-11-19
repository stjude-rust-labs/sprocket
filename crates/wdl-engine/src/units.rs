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
    pub fn convert(&self, bytes: u64) -> f64 {
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
