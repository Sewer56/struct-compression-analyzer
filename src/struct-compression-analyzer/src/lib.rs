#![doc = include_str!(concat!("../", std::env!("CARGO_PKG_README")))]

pub mod analyzer;
pub mod brute_force;
pub mod comparison;
pub mod csv;
pub mod offset_evaluator;
pub mod plot;
pub mod results;
pub mod schema;
pub mod utils;
