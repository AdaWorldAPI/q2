//! SIMD-accelerated N-dimensional arrays for Quarto graph notebooks.
//!
//! By default, this crate re-exports the [`ndarray`] fork from AdaWorldAPI
//! which includes AVX-512 / AVX2 / scalar SIMD dispatch.
//!
//! Commercial users who cannot import the fork can disable default features:
//! ```toml
//! q2-ndarray = { path = "...", default-features = false }
//! ```
//! This gives a minimal pure-Rust fallback with the same `Array2D` API
//! but no SIMD acceleration or ndarray dependency.

// ── Feature: ndarray-simd (default) ─────────────────────────────────────────

#[cfg(feature = "ndarray-simd")]
pub use ndarray;

#[cfg(feature = "ndarray-simd")]
pub use ndarray::{Array, Array1, Array2, ArrayView1, ArrayView2, Axis, Ix1, Ix2};

/// Convenience alias: a 2D f64 array backed by ndarray (SIMD path).
#[cfg(feature = "ndarray-simd")]
pub type Array2D = ndarray::Array2<f64>;

/// Create a 2D zero-filled array (SIMD path).
#[cfg(feature = "ndarray-simd")]
pub fn zeros(rows: usize, cols: usize) -> Array2D {
    ndarray::Array2::zeros((rows, cols))
}

/// Create a 2D one-filled array (SIMD path).
#[cfg(feature = "ndarray-simd")]
pub fn ones(rows: usize, cols: usize) -> Array2D {
    ndarray::Array2::ones((rows, cols))
}

/// Build a 2D array from a flat `Vec<f64>` (SIMD path).
#[cfg(feature = "ndarray-simd")]
pub fn from_vec(rows: usize, cols: usize, data: Vec<f64>) -> Result<Array2D, String> {
    ndarray::Array2::from_shape_vec((rows, cols), data)
        .map_err(|e| format!("Shape error: {e}"))
}

// ── Fallback: pure-Rust scalar (no ndarray dependency) ──────────────────────

#[cfg(not(feature = "ndarray-simd"))]
mod fallback;

#[cfg(not(feature = "ndarray-simd"))]
pub use fallback::*;
