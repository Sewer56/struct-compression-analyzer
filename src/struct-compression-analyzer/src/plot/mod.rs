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
    output_path: &Path,
) -> Result<(), PlotError<'a>> {
    if results.is_empty() || results[0].split_comparisons.is_empty() {
        return Ok(()); // No data to plot
    }

    let root = create_drawing_area(results, output_path)?;

    // Create the chart.
    let mut chart = create_ratio_chart(results.len(), &root)?;

    // Add labels (file indices).
    draw_ratio_grid(results.len(), &mut chart)?;

    // Draw the lines
    let line_style = ShapeStyle::from(BLUE).stroke_width(5);
    let coord_style = ShapeStyle::from(BLACK).filled();
    for comp_idx in 0..results[0].split_comparisons.len() {
        let mut data_points: Vec<(f64, f64)> = Vec::new();
        for (file_idx, result) in results.iter().enumerate() {
            let comparison_result = &result.split_comparisons[comp_idx];
            let base_zstd = comparison_result.group1_metrics.zstd_size;
            let compare_zstd = comparison_result.group2_metrics.zstd_size;
            data_points.push((file_idx as f64, calc_ratio_f64(compare_zstd, base_zstd)));
        }

        chart
            .draw_series(LineSeries::new(data_points.clone(), line_style))?
            .label("zstd_ratio")
            .legend(|(x, y)| {
                PathElement::new(
                    vec![(x, y), (x + 20, y)],
                    ShapeStyle::from(&BLUE).stroke_width(5),
                )
            });
        chart.draw_series(PointSeries::<_, _, Circle<_, _>, _>::new(
            data_points,
            7.5,
            coord_style,
        ))?;
    }

    add_series_labels(&mut chart)?;
    root.present()?;
    Ok(())
}

fn create_drawing_area<'a, 'b>(
    results: &[AnalysisResults],
    output_file: &'b Path,
) -> Result<DrawingArea<BitMapBackend<'b>, plotters::coord::Shift>, PlotError<'a>> {
    // Auto adjust size such that each value has constant amount of sapce.
    let width = results.len() * 64;
    let root = BitMapBackend::new(output_file, (width as u32, 1440)).into_drawing_area();
    root.fill(&WHITE)?;
    Ok(root)
}

/// Creates a chart for plotting compression ratio information,
/// with a fixed range of 0.75 to 1.25 in terms of compression ratio.
fn create_ratio_chart<'a, 'b>(
    num_results: usize,
    root: &DrawingArea<BitMapBackend<'b>, plotters::coord::Shift>,
) -> Result<
    ChartContext<
        'b,
        BitMapBackend<'b>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    PlotError<'a>,
> {
    let chart: ChartContext<
        '_,
        BitMapBackend<'b>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    > = ChartBuilder::on(root)
        .caption("Zstd Ratio", ("sans-serif", 50).into_font())
        .margin(5)
        .x_label_area_size(80)
        .y_label_area_size(80)
        .build_cartesian_2d(
            0f64..num_results as f64, // x axis range, one point per file
            0.75f64..1.25f64,         // y axis range, adjust as needed
        )?;
    Ok(chart)
}

/// Draws the grid, including the labels for a graph which presents a compression ratio
/// centered around 1.0
fn draw_ratio_grid<'a, 'b>(
    results_len: usize,
    chart: &mut ChartContext<
        'b,
        BitMapBackend<'b>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
) -> Result<(), PlotError<'a>> {
    chart
        .configure_mesh()
        // Title
        .axis_desc_style(("sans-serif", 40).into_font())
        // y labels
        .y_label_style(("sans-serif", 40).into_font())
        // x labels
        .x_labels(results_len)
        .x_label_style(("sans-serif", 40).into_font())
        .x_label_formatter(&|x| format!("{}", x))
        .draw()?;
    Ok(())
}

/// Adds the series labels to the current chart.
/// i.e. the little box which shows lines and their corresponding names.
fn add_series_labels<'a, 'b>(
    chart: &mut ChartContext<
        'b,
        BitMapBackend<'b>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
) -> Result<(), PlotError<'a>> {
    chart
        .configure_series_labels()
        .label_font(("sans-serif", 40))
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .position(SeriesLabelPosition::UpperLeft)
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
