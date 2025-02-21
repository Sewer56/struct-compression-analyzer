//! Generates plots for analysis results.
//!
//! This module provides functions to create various plots based on the analysis
//! results, using the `plotters` crate.

use crate::analysis_results::AnalysisResults;
use plotters::prelude::*;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use thiserror::Error;

/// Custom error type for plot generation.
#[derive(Error, Debug)]
pub enum PlotError<'a> {
    /// Error during file creation or writing.
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Count of file paths {found} does not match the count of analysis results {expected}")]
    InvalidFileCount { found: usize, expected: usize },

    /// Error parsing data for plotting.
    #[error("Drawing area error: {0}")]
    DrawingAreaError(
        #[from] DrawingAreaErrorKind<<BitMapBackend<'a> as DrawingBackend>::ErrorType>,
    ),
}

/// Generates a line plot for the "ratio_zstd" column from split comparisons.
///
/// This function creates a PNG file in the specified output directory,
/// visualizing the "ratio_zstd" values for each split comparison across
/// all analyzed files as a line graph.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `output_dir` - The directory where the plot file will be written.
/// * `files` - A slice of [`PathBuf`]s representing the original file paths for each result.
///
/// # Returns
///
/// * `Result<(), PlotError>` - Ok if successful, otherwise a `PlotError`.
pub fn generate_split_comparison_zstd_ratio_plot<'a>(
    results: &[AnalysisResults],
    output_dir: &Path,
    files: &[PathBuf],
) -> Result<(), PlotError<'a>> {
    if results.is_empty() || results[0].split_comparisons.is_empty() {
        return Ok(()); // No data to plot
    }

    if files.len() != results.len() {
        return Err(PlotError::InvalidFileCount {
            found: files.len(),
            expected: results.len(),
        });
    }

    let output_file = output_dir.join("split_comparison_zstd_ratio.png");

    // Create the image.
    let root = BitMapBackend::new(&output_file, (2560, 1440)).into_drawing_area();
    root.fill(&WHITE)?;

    // Create the chart.
    let mut chart = ChartBuilder::on(&root)
        .caption("Zstd Ratio", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(80)
        .y_label_area_size(80)
        .build_cartesian_2d(
            0f64..results.len() as f64, // x axis range, one point per file
            0f64..2.0f64,               // y axis range, adjust as needed
        )?;

    // Add labels (filenames) - this is basic, might need improvement for many files
    chart
        .configure_mesh()
        // Title
        .axis_desc_style(("sans-serif", 40).into_font())
        // y labels
        .y_label_style(("sans-serif", 40).into_font())
        // x labels
        .x_labels(results.len())
        .x_label_style(("sans-serif", 40).into_font())
        .x_label_formatter(&|x| format!("{}", x))
        .draw()?;

    // Draw the lines
    let line_style = ShapeStyle::from(&BLUE).stroke_width(5);
    let coord_style = ShapeStyle::from(&BLACK).filled();
    for comp_idx in 0..results[0].split_comparisons.len() {
        let mut data_points: Vec<(f64, f64)> = Vec::new();
        for (file_idx, result) in results.iter().enumerate() {
            let comparison_result = &result.split_comparisons[comp_idx];
            let base_zstd = comparison_result.group1_metrics.zstd_size;
            let compare_zstd = comparison_result.group2_metrics.zstd_size;
            data_points.push((file_idx as f64, calc_ratio_f64(compare_zstd, base_zstd)));
        }

        chart.draw_series(LineSeries::new(data_points.clone(), line_style))?;
        chart.draw_series(PointSeries::<_, _, Circle<_, _>, _>::new(
            data_points,
            7.5,
            coord_style,
        ))?;
    }

    Ok(())
}

/// Calculates a ratio between two numbers, handling division by zero.
///
/// # Arguments
///
/// * `child` - The numerator. (comparison)
/// * `parent` - The denominator. (base)
///
/// # Returns
///
/// A string representing the ratio, or "0.0" if the denominator is zero.
fn calc_ratio_f64(child: u64, parent: u64) -> f64 {
    if parent == 0 {
        0.0
    } else {
        child as f64 / parent as f64
    }
}
