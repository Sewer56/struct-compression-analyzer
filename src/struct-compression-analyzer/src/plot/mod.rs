//! Generates plots for analysis results.
//!
//! This module provides functions to create various plots based on the analysis
//! results, using the `plotters` crate.

use crate::analysis_results::AnalysisResults;
use plotters::prelude::*;
use std::path::Path;
use thiserror::Error;

/// Custom error type for plot generation.
#[derive(Error, Debug)]
pub enum PlotError<'a> {
    /// Error during file creation or writing.
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

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
///
/// # Returns
///
/// * `Result<(), PlotError>` - Ok if successful, otherwise a `PlotError`.
pub fn generate_split_comparison_zstd_ratio_plot<'a>(
    results: &[AnalysisResults],
    output_dir: &Path,
) -> Result<(), PlotError<'a>> {
    if results.is_empty() || results[0].split_comparisons.is_empty() {
        return Ok(()); // No data to plot
    }

    let output_file = output_dir.join("split_comparison_zstd_ratio.png");
    let root = BitMapBackend::new(&output_file, (1280, 720)).into_drawing_area();
    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        .caption("Zstd Ratio", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(
            0f64..results.len() as f64, // x axis range, one point per file
            0f64..2.0f64,               // y axis range, adjust as needed
        )?;

    chart.configure_mesh().draw()?;

    for comp_idx in 0..results[0].split_comparisons.len() {
        let mut data_points: Vec<(f64, f64)> = Vec::new();
        for (file_idx, result) in results.iter().enumerate() {
            let comparison_result = &result.split_comparisons[comp_idx];
            let base_zstd = comparison_result.group1_metrics.zstd_size;
            let compare_zstd = comparison_result.group2_metrics.zstd_size;
            data_points.push((file_idx as f64, calc_ratio_f64(compare_zstd, base_zstd)));
        }

        chart.draw_series(LineSeries::new(data_points, &BLUE))?;
    }

    // Add x-axis labels (filenames) - this is basic, might need improvement for many files
    chart
        .configure_mesh()
        .axis_desc_style(("sans-serif", 10).into_font())
        .x_labels(results.len() as usize)
        .x_label_formatter(&|x| {
            let int_x = *x as usize;
            if int_x < results.len() {
                format!("File {}", int_x)
            } else {
                String::new()
            }
        })
        .draw()?;

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
