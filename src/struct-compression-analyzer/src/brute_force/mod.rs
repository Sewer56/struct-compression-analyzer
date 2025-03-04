//! Brute force optimization for LZ match and entropy multiplier parameters.
//!
//! This module provides functionality to find optimal values for the
//! [`lz_match_multiplier`] and [`entropy_multiplier`] parameters used in the
//! [`size_estimate`] function.
//!
//! [`size_estimate`]: crate::utils::analyze_utils::size_estimate
//! [`lz_match_multiplier`]: crate::analyzer::SizeEstimationParameters::lz_match_multiplier
//! [`entropy_multiplier`]: crate::analyzer::SizeEstimationParameters::entropy_multiplier

pub mod brute_force_split;

/// Configuration for the brute force optimization process.
#[derive(Debug, Clone)]
pub struct BruteForceConfig {
    /// Minimum value for LZ match multiplier
    pub min_lz_multiplier: f64,
    /// Maximum value for LZ match multiplier
    pub max_lz_multiplier: f64,
    /// Step size for LZ match multiplier
    pub lz_step_size: f64,
    /// Minimum value for entropy multiplier
    pub min_entropy_multiplier: f64,
    /// Maximum value for entropy multiplier
    pub max_entropy_multiplier: f64,
    /// Step size for entropy multiplier
    pub entropy_step_size: f64,
}

impl Default for BruteForceConfig {
    fn default() -> Self {
        Self {
            min_lz_multiplier: 0.001,
            max_lz_multiplier: 1.0,
            lz_step_size: 0.001,
            min_entropy_multiplier: 1.0,
            max_entropy_multiplier: 1.2,
            entropy_step_size: 0.001,
        }
    }
}

/// Result of a brute force optimization.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizationResult {
    /// Optimized LZ match multiplier
    pub lz_match_multiplier: f64,
    /// Optimized entropy multiplier
    pub entropy_multiplier: f64,
}
