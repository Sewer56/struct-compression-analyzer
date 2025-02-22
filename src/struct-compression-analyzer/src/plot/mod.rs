//! Generates plots for analysis results.
//!
//! This module provides functions to create various plots based on the analysis
//! results, using the `plotters` crate.

use crate::{
    analysis_results::AnalysisResults, comparison::split_comparison::SplitComparisonResult,
};
use plotters::prelude::*;
use std::path::Path;

/// Struct to hold data and styling for a single plot line.
struct PlotData<'a> {
    label: &'a str,
    line_color: RGBColor,
    data_points: Vec<(f64, f64)>,
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
/// * `Result<(), Box<dyn std::error::Error>>` - Ok if successful, otherwise a boxed `std::error::Error`.
pub fn generate_split_comparison_plot(
    results: &[AnalysisResults],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if results.is_empty() || results[0].split_comparisons.is_empty() {
        return Ok(()); // No data to plot
    }

    let root = create_drawing_area(results, output_path)?;

    // Create the chart.
    let mut chart = create_ratio_chart(results.len(), &root)?;

    // Add labels (file indices).
    draw_ratio_grid(results.len(), &mut chart)?;

    // Prepare plot data
    let mut plots: Vec<PlotData> = Vec::new();

    // Zstd Ratio Plot Data
    for comp_idx in 0..results[0].split_comparisons.len() {
        let zstd_data_points = calculate_plot_data_points(results, comp_idx, |comparison| {
            let base_zstd = comparison.group1_metrics.zstd_size;
            let compare_zstd = comparison.group2_metrics.zstd_size;
            calc_ratio_f64(compare_zstd, base_zstd)
        });

        plots.push(PlotData {
            label: "zstd_ratio",
            line_color: BLUE,
            data_points: zstd_data_points,
        });
    }

    // LZ Ratio Plot Data (inverted)
    for comp_idx in 0..results[0].split_comparisons.len() {
        let lz_data_points = calculate_plot_data_points(results, comp_idx, |comparison| {
            let base_lz = comparison.group1_metrics.lz_matches;
            let compare_lz = comparison.group2_metrics.lz_matches;
            1.0 / calc_ratio_f64(compare_lz, base_lz)
        });
        plots.push(PlotData {
            label: "1 / lz_matches",
            line_color: RED,
            data_points: lz_data_points,
        });
    }

    // Entropy Difference Plot Data
    for comp_idx in 0..results[0].split_comparisons.len() {
        let lz_data_points = calculate_plot_data_points(results, comp_idx, |comparison| {
            1.0 / comparison.split_max_entropy_diff_ratio()
        });
        plots.push(PlotData {
            label: "1 / entropy_ratio",
            line_color: GREEN,
            data_points: lz_data_points,
        });
    }

    // Draw plots
    for plot in plots {
        draw_plot(&mut chart, &plot)?;
    }

    add_series_labels(&mut chart)?;
    root.present()?;
    Ok(())
}

/// Calculates the data points for a plot.
fn calculate_plot_data_points<F>(
    results: &[AnalysisResults],
    comp_idx: usize,
    value_calculator: F,
) -> Vec<(f64, f64)>
where
    F: Fn(&SplitComparisonResult) -> f64,
{
    let mut data_points: Vec<(f64, f64)> = Vec::new();
    for (file_idx, result) in results.iter().enumerate() {
        let comparison_result = &result.split_comparisons[comp_idx];
        let y_value = value_calculator(comparison_result);
        data_points.push((file_idx as f64, y_value));
    }
    data_points
}

/// Draws a single plot line and its points.
fn draw_plot<'a>(
    chart: &mut ChartContext<
        'a,
        BitMapBackend<'a>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    plot: &PlotData,
) -> Result<(), Box<dyn std::error::Error>> {
    let line_color = plot.line_color;
    let line_style = ShapeStyle::from(line_color).stroke_width(5);
    let coord_style = ShapeStyle::from(BLACK).filled();

    let plot_points = plot.data_points.clone();
    chart
        .draw_series(LineSeries::new(plot_points, line_style))?
        .label(plot.label)
        .legend(move |(x, y)| {
            PathElement::new(
                vec![(x, y), (x + 20, y)],
                ShapeStyle::from(line_color).stroke_width(5),
            )
        });

    chart.draw_series(PointSeries::<_, _, Circle<_, _>, _>::new(
        plot.data_points.clone(),
        7.5,
        coord_style,
    ))?;

    Ok(())
}

fn create_drawing_area<'a>(
    results: &[AnalysisResults],
    output_file: &'a Path,
) -> Result<DrawingArea<BitMapBackend<'a>, plotters::coord::Shift>, Box<dyn std::error::Error>> {
    // Auto adjust size such that each value has constant amount of sapce.
    let width = results.len() * 64;
    let root = BitMapBackend::new(output_file, (width as u32, 1440)).into_drawing_area();
    root.fill(&WHITE)?;
    Ok(root)
}

/// Creates a chart for plotting compression ratio information,
/// with a fixed range of 0.6 to 1.20 in terms of compression ratio.
fn create_ratio_chart<'a>(
    num_results: usize,
    root: &DrawingArea<BitMapBackend<'a>, plotters::coord::Shift>,
) -> Result<
    ChartContext<
        'a,
        BitMapBackend<'a>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
    Box<dyn std::error::Error>,
> {
    let chart: ChartContext<
        '_,
        BitMapBackend<'a>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    > = ChartBuilder::on(root)
        .margin(5)
        .x_label_area_size(80)
        .y_label_area_size(80)
        .build_cartesian_2d(
            0f64..num_results as f64, // x axis range, one point per file
            0.60f64..1.20f64,         // y axis range, adjust as needed
        )?;
    Ok(chart)
}

/// Draws the grid, including the labels for a graph which presents a compression ratio
/// centered around 1.0
fn draw_ratio_grid<'a>(
    results_len: usize,
    chart: &mut ChartContext<
        'a,
        BitMapBackend<'a>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
) -> Result<(), Box<dyn std::error::Error>> {
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
fn add_series_labels<'a>(
    chart: &mut ChartContext<
        'a,
        BitMapBackend<'a>,
        Cartesian2d<plotters::coord::types::RangedCoordf64, plotters::coord::types::RangedCoordf64>,
    >,
) -> Result<(), Box<dyn std::error::Error>> {
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
