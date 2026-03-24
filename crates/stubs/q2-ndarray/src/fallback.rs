//! Pure-Rust scalar fallback for commercial deployments that cannot import
//! the AdaWorldAPI/ndarray fork. No SIMD, no external dependencies.

/// A 2-dimensional array of f64 values (scalar fallback).
#[derive(Debug, Clone)]
pub struct Array2D {
    data: Vec<f64>,
    rows: usize,
    cols: usize,
}

impl Array2D {
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![0.0; rows * cols],
            rows,
            cols,
        }
    }

    pub fn ones(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![1.0; rows * cols],
            rows,
            cols,
        }
    }

    pub fn from_vec(rows: usize, cols: usize, data: Vec<f64>) -> Result<Self, String> {
        if data.len() != rows * cols {
            return Err(format!(
                "Data length {} does not match dimensions {}x{}",
                data.len(),
                rows,
                cols
            ));
        }
        Ok(Self { data, rows, cols })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn get(&self, row: usize, col: usize) -> Option<f64> {
        if row < self.rows && col < self.cols {
            Some(self.data[row * self.cols + col])
        } else {
            None
        }
    }

    pub fn set(&mut self, row: usize, col: usize, value: f64) -> Result<(), String> {
        if row < self.rows && col < self.cols {
            self.data[row * self.cols + col] = value;
            Ok(())
        } else {
            Err(format!(
                "Index ({}, {}) out of bounds for {}x{} array",
                row, col, self.rows, self.cols
            ))
        }
    }

    pub fn add(&self, other: &Array2D) -> Result<Array2D, String> {
        if self.shape() != other.shape() {
            return Err("Shape mismatch".to_string());
        }
        let data: Vec<f64> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a + b)
            .collect();
        Ok(Array2D {
            data,
            rows: self.rows,
            cols: self.cols,
        })
    }

    pub fn mul(&self, other: &Array2D) -> Result<Array2D, String> {
        if self.shape() != other.shape() {
            return Err("Shape mismatch".to_string());
        }
        let data: Vec<f64> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .collect();
        Ok(Array2D {
            data,
            rows: self.rows,
            cols: self.cols,
        })
    }

    pub fn matmul(&self, other: &Array2D) -> Result<Array2D, String> {
        if self.cols != other.rows {
            return Err(format!(
                "Cannot multiply {}x{} by {}x{}",
                self.rows, self.cols, other.rows, other.cols
            ));
        }
        let mut result = Array2D::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for j in 0..other.cols {
                let mut sum = 0.0;
                for k in 0..self.cols {
                    sum += self.data[i * self.cols + k] * other.data[k * other.cols + j];
                }
                result.data[i * other.cols + j] = sum;
            }
        }
        Ok(result)
    }

    pub fn as_slice(&self) -> &[f64] {
        &self.data
    }
}

/// Create a 2D zero-filled array (scalar fallback).
pub fn zeros(rows: usize, cols: usize) -> Array2D {
    Array2D::zeros(rows, cols)
}

/// Create a 2D one-filled array (scalar fallback).
pub fn ones(rows: usize, cols: usize) -> Array2D {
    Array2D::ones(rows, cols)
}

/// Build a 2D array from a flat `Vec<f64>` (scalar fallback).
pub fn from_vec(rows: usize, cols: usize, data: Vec<f64>) -> Result<Array2D, String> {
    Array2D::from_vec(rows, cols, data)
}
