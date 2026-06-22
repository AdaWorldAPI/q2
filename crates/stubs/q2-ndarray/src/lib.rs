//! SIMD-accelerated N-dimensional arrays for Quarto graph notebooks.
//!
//! By default, this crate re-exports the [`ndarray`] fork from AdaWorldAPI
//! which includes AVX-512 / AVX2 / scalar SIMD dispatch across 57 HPC modules.
//!
//! Commercial users who cannot import the fork can disable default features:
//! ```toml
//! q2-ndarray = { path = "...", default-features = false }
//! ```
//! This gives a minimal pure-Rust fallback with the same `Array2D` API
//! but no SIMD acceleration or ndarray dependency.

// ── DeepNSM semantic analysis (always available, no feature gate) ────────────

pub mod deepnsm;

// ── Feature: ndarray-simd (default) ─────────────────────────────────────────

#[cfg(feature = "ndarray-simd")]
pub use ndarray;

// ── Core array types ────────────────────────────────────────────────────────

#[cfg(feature = "ndarray-simd")]
pub use ndarray::{Array, Array1, Array2, ArrayView1, ArrayView2, Axis, Ix1, Ix2};

/// Convenience alias: a 2D f64 array backed by ndarray (SIMD path).
#[cfg(feature = "ndarray-simd")]
pub type Array2D = ndarray::Array2<f64>;

// ── SIMD dispatch: crate::simd re-exports ───────────────────────────────────
//
// The ndarray fork exposes three tiers of SIMD access:
//
//   ndarray::simd_avx2   — pub mod, AVX2 intrinsics (dot, hamming, popcount)
//   ndarray::backend     — pub dispatch layer (dot_f32, axpy_f32, etc.)
//   ndarray::hpc         — 57 pub HPC modules (blas_level1-3, graph, fft, etc.)
//
// The lower-level simd.rs (portable types: F32x16, F64x8) and simd_avx512.rs
// are pub(crate) inside ndarray, so we access them through the backend/hpc
// dispatch layer which auto-selects AVX-512 → AVX2 → scalar at runtime.

/// SIMD-accelerated AVX2 primitives (dot products, Hamming distance, popcount).
///
/// Use as `q2_ndarray::simd::dot_f32(...)` etc.
#[cfg(all(feature = "ndarray-simd", target_arch = "x86_64"))]
pub use ndarray::simd_avx2 as simd;

/// BLAS-level dispatch layer — auto-selects fastest SIMD backend at runtime.
///
/// Provides `dot_f32`, `dot_f64`, `axpy_f32`, `axpy_f64`, `scal_*`, `nrm2_*`, `asum_*`.
/// Use as `q2_ndarray::backend::dot_f32(...)`.
#[cfg(feature = "ndarray-simd")]
pub use ndarray::backend;

/// Full HPC module suite — 57 modules including BLAS L1-L3, FFT, graph ops,
/// hyperdimensional computing, NARS, fingerprinting, and more.
///
/// Use as `q2_ndarray::hpc::blas_level1::BlasLevel1` etc.
#[cfg(feature = "ndarray-simd")]
pub use ndarray::hpc;

// ── Convenience constructors (SIMD path) ────────────────────────────────────

/// Create a 2D zero-filled array.
#[cfg(feature = "ndarray-simd")]
pub fn zeros(rows: usize, cols: usize) -> Array2D {
    ndarray::Array2::zeros((rows, cols))
}

/// Create a 2D one-filled array.
#[cfg(feature = "ndarray-simd")]
pub fn ones(rows: usize, cols: usize) -> Array2D {
    ndarray::Array2::ones((rows, cols))
}

/// Build a 2D array from a flat `Vec<f64>`.
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
