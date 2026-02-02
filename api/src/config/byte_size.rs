use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSize(pub u64);

impl FromStr for ByteSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_byte_size(s)
    }
}

impl<'de> Deserialize<'de> for ByteSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ByteSizeVisitor;

        impl<'de> serde::de::Visitor<'de> for ByteSizeVisitor {
            type Value = ByteSize;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a byte size as number or string with suffix (KB, MB, GB, TB)")
            }

            fn visit_i64<E>(self, value: i64) -> Result<ByteSize, E>
            where
                E: serde::de::Error,
            {
                if value < 0 {
                    return Err(E::custom("byte size cannot be negative"));
                }
                Ok(ByteSize(value as u64))
            }

            fn visit_u64<E>(self, value: u64) -> Result<ByteSize, E>
            where
                E: serde::de::Error,
            {
                Ok(ByteSize(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<ByteSize, E>
            where
                E: serde::de::Error,
            {
                parse_byte_size(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(ByteSizeVisitor)
    }
}

fn parse_byte_size(s: &str) -> Result<ByteSize, String> {
    let s = s.trim();
    let s_upper = s.to_uppercase();

    let (num_part, multiplier) = if let Some(value) = s_upper.strip_suffix("TB") {
        (value, 1000u64.pow(4))
    } else if let Some(value) = s_upper.strip_suffix("TIB") {
        (value, 1024u64.pow(4))
    } else if let Some(value) = s_upper.strip_suffix("GB") {
        (value, 1000u64.pow(3))
    } else if let Some(value) = s_upper.strip_suffix("GIB") {
        (value, 1024u64.pow(3))
    } else if let Some(value) = s_upper.strip_suffix("MB") {
        (value, 1000u64.pow(2))
    } else if let Some(value) = s_upper.strip_suffix("MIB") {
        (value, 1024u64.pow(2))
    } else if let Some(value) = s_upper.strip_suffix("KB") {
        (value, 1000u64)
    } else if let Some(value) = s_upper.strip_suffix("KIB") {
        (value, 1024u64)
    } else {
        (s, 1)
    };

    let num: u64 = num_part
        .trim()
        .parse()
        .map_err(|e| format!("Invalid number '{}': {}", num_part, e))?;

    num.checked_mul(multiplier)
        .map(ByteSize)
        .ok_or_else(|| "Byte size overflow".to_string())
}


impl ByteSize {
    pub fn as_bytes(&self) -> u64 {
        self.0
    }

    // Binary units (base 1024)
    pub fn as_kib(&self) -> f64 {
        self.0 as f64 / 1024.0
    }

    pub fn as_mib(&self) -> f64 {
        self.0 as f64 / (1024.0 * 1024.0)
    }

    pub fn as_gib(&self) -> f64 {
        self.0 as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn as_tib(&self) -> f64 {
        self.0 as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0)
    }

    // Decimal units (base 1000)
    pub fn as_kb(&self) -> f64 {
        self.0 as f64 / 1000.0
    }

    pub fn as_mb(&self) -> f64 {
        self.0 as f64 / (1000.0 * 1000.0)
    }

    pub fn as_gb(&self) -> f64 {
        self.0 as f64 / (1000.0 * 1000.0 * 1000.0)
    }

    pub fn as_tb(&self) -> f64 {
        self.0 as f64 / (1000.0 * 1000.0 * 1000.0 * 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_units() {
        assert_eq!(parse_byte_size("1KB").unwrap().0, 1_000);
        assert_eq!(parse_byte_size("1MB").unwrap().0, 1_000_000);
        assert_eq!(parse_byte_size("1GB").unwrap().0, 1_000_000_000);
        assert_eq!(parse_byte_size("1TB").unwrap().0, 1_000_000_000_000);
    }

    #[test]
    fn test_binary_units() {
        assert_eq!(parse_byte_size("1KiB").unwrap().0, 1_024);
        assert_eq!(parse_byte_size("1MiB").unwrap().0, 1_048_576);
        assert_eq!(parse_byte_size("1GiB").unwrap().0, 1_073_741_824);
        assert_eq!(parse_byte_size("1TiB").unwrap().0, 1_099_511_627_776);
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(parse_byte_size("1gib").unwrap().0, 1_073_741_824);
        assert_eq!(parse_byte_size("1GiB").unwrap().0, 1_073_741_824);
        assert_eq!(parse_byte_size("1GIB").unwrap().0, 1_073_741_824);
    }

    #[test]
    fn test_difference() {
        // Mostra la differenza tra decimale e binario
        let decimal_gb = parse_byte_size("100GB").unwrap().0;
        let binary_gib = parse_byte_size("100GiB").unwrap().0;

        assert_eq!(decimal_gb, 100_000_000_000);
        assert_eq!(binary_gib, 107_374_182_400);

        // ~7.4% di differenza
        let diff_percent = ((binary_gib - decimal_gb) as f64 / decimal_gb as f64) * 100.0;
        assert!((diff_percent - 7.37).abs() < 0.01);
    }
}