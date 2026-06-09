//! Error type for brand parsing and resolution.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrandError {
    /// YAML parse / deserialization failure (covers unknown fields too).
    #[error("failed to parse _brand.yml: {0}")]
    Parse(#[from] serde_yaml::Error),

    /// Color name aliasing formed a cycle longer than 100 steps.
    #[error("circular reference in _brand.yml color definitions: {chain}")]
    CircularColorReference { chain: String },

    /// A color reference resolved to a name that isn't a valid CSS color
    /// and isn't aliased in the palette.
    #[error("unknown color name in _brand.yml: {name}")]
    UnknownColorName { name: String },
}
