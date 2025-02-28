//! Generates plots for analysis results.
//!
//! This module provides functions to create various plots based on the analysis
//! results, using the `plotters` crate.

use crate::comparison::{
    compare_groups::GroupComparisonResult, split_comparison::SplitComparisonResult,
};
use crate::results::analysis_results::AnalysisResults;
use core::{error::Error, ops::Range};
use plotters::{prelude::*, style::full_palette::PURPLE};
use std::{fs, path::Path};

/// Generates all plots for the analysis results.
///
/// This function acts as a wrapper to generate multiple plots,
/// including the split comparison plot.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `output_dir` - The directory where the plot files will be written.
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - Ok if successful, otherwise a boxed [`std::error::Error`].
pub fn generate_plots(
    results: &[AnalysisResults],
    output_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if results.is_empty() {
        return Ok(());
    }

    let split_compare_dir = output_dir.join("split_comparison_plots");
    fs::create_dir_all(&split_compare_dir)?;

    // Generate split comparison plot
    for (x, comparison) in results[0].split_comparisons.iter().enumerate() {
        let output_path = split_compare_dir.join(format!("{}.png", comparison.name));
        generate_ratio_split_comparison_plot(results, x, &output_path, false, false)?;

        let output_path = split_compare_dir.join(format!("{}_with_estimate.png", comparison.name));
        generate_ratio_split_comparison_plot(results, x, &output_path, false, true)?;

        let output_path =
            split_compare_dir.join(format!("{}_with_entropy_by_lzmatches.png", comparison.name));
        generate_ratio_split_comparison_plot(results, x, &output_path, true, false)?;
    }

    let custom_comparisons_dir = output_dir.join("custom_comparison_plots");
    fs::create_dir_all(&custom_comparisons_dir)?;

    // Generate custom comparison plot
    // Note: Assumption all items have same number of comparisons and
    for (x, comparison) in results[0].custom_comparisons.iter().enumerate() {
        // Write data for individual groups.
        for (y, group_name) in comparison.group_names.iter().enumerate() {
            let output_path = custom_comparisons_dir.join(format!(
                "{}_{}_{}.png",
                comparison.name,
                group_name.replace(' ', "_"),
                y
            ));
            generate_ratio_custom_comparison_plot(results, x, y..y + 1, &output_path, false)?;

            let output_path = custom_comparisons_dir.join(format!(
                "{}_{}_{}_with_estimate.png",
                comparison.name,
                group_name.replace(' ', "_"),
                y
            ));
            generate_ratio_custom_comparison_plot(results, x, y..y + 1, &output_path, true)?;
        }

        let output_path = custom_comparisons_dir.join(format!("{}.png", comparison.name));
        generate_ratio_custom_comparison_plot(
            results,
            x,
            0..comparison.group_names.len(),
            &output_path,
            false,
        )?;

        let output_path =
            custom_comparisons_dir.join(format!("{}_with_estimate.png", comparison.name));
        generate_ratio_custom_comparison_plot(
            results,
            x,
            0..comparison.group_names.len(),
            &output_path,
            true,
        )?;
    }

    // Add calls to other plot generation functions here in the future
    Ok(())
}

/// Struct to hold data and styling for a single plot line.
struct PlotData {
    label: String,
    line_color: RGBColor,
    data_points: Vec<(f64, f64)>,
}

/// Generates a line plot for the various columns from a split comparison.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `comparison_index` - The index of the split comparison to plot in the `split_comparisons` array.
/// * `output_path` - The path where the plot file will be written.
/// * `include_entropy_by_lzmatches_column` - Includes column for (1 / lz_matches * entropy_ratio).
/// * `include_estimate_column` - Includes column for (estimate_ratio).
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - Ok if successful, otherwise a boxed [`std::error::Error`].
pub fn generate_ratio_split_comparison_plot(
    results: &[AnalysisResults],
    comparison_index: usize,
    output_path: &Path,
    include_entropy_by_lzmatches_column: bool,
    include_estimate_column: bool,
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
    let zstd_data_points = make_split_data_points(results, comparison_index, |comparison| {
        let base_zstd = comparison.group1_metrics.zstd_size;
        let compare_zstd = comparison.group2_metrics.zstd_size;
        calc_ratio_f64(compare_zstd, base_zstd)
    });

    plots.push(PlotData {
        label: "zstd_ratio".to_owned(),
        line_color: BLACK,
        data_points: zstd_data_points,
    });

    // LZ Ratio Plot Data (inverted)
    let lz_data_points = make_split_data_points(results, comparison_index, |comparison| {
        let base_lz = comparison.group1_metrics.lz_matches;
        let compare_lz = comparison.group2_metrics.lz_matches;
        1.0 / calc_ratio_f64(compare_lz, base_lz)
    });

    plots.push(PlotData {
        label: "1 / lz_matches_ratio".to_owned(),
        line_color: RED,
        data_points: lz_data_points,
    });

    // Entropy Difference Plot Data
    let lz_data_points = make_split_data_points(results, comparison_index, |comparison| {
        1.0 / comparison.split_max_entropy_diff_ratio()
    });

    plots.push(PlotData {
        label: "1 / entropy_ratio".to_owned(),
        line_color: GREEN,
        data_points: lz_data_points,
    });

    if include_entropy_by_lzmatches_column {
        let data_points = make_split_data_points(results, comparison_index, |comparison| {
            let base_lz = comparison.group1_metrics.lz_matches;
            let compare_lz = comparison.group2_metrics.lz_matches;
            let lz_matches_ratio = calc_ratio_f64(compare_lz, base_lz);
            1.0 / (comparison.split_max_entropy_diff_ratio() * lz_matches_ratio)
        });

        plots.push(PlotData {
            label: "1 / (entropy_ratio * lz_matches)".to_owned(),
            line_color: BLUE,
            data_points,
        });
    }

    if include_estimate_column {
        // LZ Ratio Plot Data (inverted)
        let data_points = make_split_data_points(results, comparison_index, |comparison| {
            let base_est = comparison.group1_metrics.estimated_size;
            let compare_est = comparison.group2_metrics.estimated_size;
            calc_ratio_f64(compare_est, base_est)
        });

        plots.push(PlotData {
            label: "estimate_ratio".to_owned(),
            line_color: PURPLE,
            data_points,
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

/// Generates the base colours that will be transformed by a gradient
fn generate_base_colors(
    num_colors: usize,
) -> Result<Vec<(RGBColor, RGBColor)>, Box<dyn std::error::Error>> {
    let mut colours = Vec::<(RGBColor, RGBColor)>::new();
    if num_colors > 0 {
        colours.push((RGBColor(0, 0, 0), RGBColor(150, 150, 150))); // Black to light grey
    }
    if num_colors > 1 {
        colours.push((RGBColor(255, 0, 0), RGBColor(255, 150, 150))); // Red to light red
    }
    if num_colors > 2 {
        colours.push((RGBColor(0, 255, 0), RGBColor(150, 255, 150))); // Green to light green
    }
    if num_colors > 3 {
        colours.push((RGBColor(0, 0, 255), RGBColor(150, 150, 255))); // Blue to light blue
    }
    if num_colors > 4 {
        return Err(Box::<dyn Error>::from(format!(
            "Too many colours: {}",
            num_colors
        )));
    }
    Ok(colours)
}

/// Generates a sequence of distinct colors for plotting, with gradients.
/// The colours are interleaved, R,G,B * num_gradients
fn generate_color_palette(
    base_colors: &[(RGBColor, RGBColor)],
    num_gradients: usize,
) -> Vec<RGBColor> {
    let mut palette = Vec::new();

    // (color channels)
    for x in 0..num_gradients {
        // Alternate, R,G,B
        for (base_color, end_color) in base_colors {
            let gradient_step = if num_gradients == 1 {
                0.0
            } else {
                x as f32 / (num_gradients - 1) as f32
            };

            let r_step = (end_color.0 as f32 - base_color.0 as f32) * gradient_step;
            let g_step = (end_color.1 as f32 - base_color.1 as f32) * gradient_step;
            let b_step = (end_color.2 as f32 - base_color.2 as f32) * gradient_step;
            let r = (base_color.0 as f32 + r_step) as u8;
            let g = (base_color.1 as f32 + g_step) as u8;
            let b = (base_color.2 as f32 + b_step) as u8;

            palette.push(RGBColor(r, g, b));
        }
    }

    palette
}

/// Generates a line plot for the various columns from a custom comparison.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `comparison_index` - The index of the custom comparison to plot in the `custom_comparisons` array.
/// * `group_indices` - The range of indices for the groups to compare.
/// * `output_path` - The path where the plot file will be written.
/// * `include_estimate_column` - Whether to include the estimate ratio column.
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - Ok if successful, otherwise a boxed [`std::error::Error`].
pub fn generate_ratio_custom_comparison_plot(
    results: &[AnalysisResults],
    comparison_index: usize,
    group_indices: Range<usize>,
    output_path: &Path,
    include_estimate_column: bool,
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
    let group_names = &results[0].custom_comparisons[0].group_names;

    // Get color palette
    let num_gradients = group_indices.len();
    let num_base_colors = 4;
    let base_colors = generate_base_colors(num_base_colors)?;
    let colors = generate_color_palette(&base_colors, num_gradients);

    // Zstd Ratio Plot Data
    let start_index = group_indices.start;
    for group_idx in group_indices {
        let group_name = &group_names[group_idx];
        let group_offset = group_idx - start_index;
        let color_offset = group_offset * num_base_colors;

        let zstd_data_points = make_custom_data_points(results, comparison_index, |comparison| {
            let base_zstd = comparison.baseline_metrics.zstd_size;
            let compare_zstd = comparison.group_metrics[group_idx].zstd_size;
            calc_ratio_f64(compare_zstd, base_zstd)
        });

        plots.push(PlotData {
            label: format!("zstd_ratio ({})", group_name),
            line_color: colors[color_offset],
            data_points: zstd_data_points,
        });

        // LZ Ratio Plot Data (inverted)
        let lz_data_points = make_custom_data_points(results, comparison_index, |comparison| {
            let base_lz = comparison.baseline_metrics.lz_matches;
            let compare_lz = comparison.group_metrics[group_idx].lz_matches;
            1.0 / calc_ratio_f64(compare_lz, base_lz)
        });

        plots.push(PlotData {
            label: format!("1 / lz_matches_ratio ({})", group_name),
            line_color: colors[color_offset + 1],
            data_points: lz_data_points,
        });

        // Entropy Ratio Plot Data
        let entropy_data_points =
            make_custom_data_points(results, comparison_index, |comparison| {
                1.0 / (comparison.baseline_metrics.entropy
                    / comparison.group_metrics[group_idx].entropy)
            });

        // Don't plot if the entropy ratio is 1.0.
        // This is a 'rough' check to avoid plotting a straight line.
        if entropy_data_points[0].1 != 1.0 {
            plots.push(PlotData {
                label: format!("entropy_ratio ({})", group_name),
                line_color: colors[color_offset + 2],
                data_points: entropy_data_points,
            });
        }

        // Estimate Ratio Plot Data
        if include_estimate_column {
            let estimate_data_points =
                make_custom_data_points(results, comparison_index, |comparison| {
                    let base_zstd = comparison.baseline_metrics.estimated_size;
                    let compare_zstd = comparison.group_metrics[group_idx].estimated_size;
                    calc_ratio_f64(compare_zstd, base_zstd)
                });

            plots.push(PlotData {
                label: format!("estimate_ratio ({})", group_name),
                line_color: colors[color_offset + 3],
                data_points: estimate_data_points,
            });
        }
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
fn make_split_data_points<F>(
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

/// Calculates the data points for a plot.
fn make_custom_data_points<F>(
    results: &[AnalysisResults],
    comp_idx: usize,
    value_calculator: F,
) -> Vec<(f64, f64)>
where
    F: Fn(&GroupComparisonResult) -> f64,
{
    let mut data_points: Vec<(f64, f64)> = Vec::new();
    for (file_idx, result) in results.iter().enumerate() {
        let comparison_result = &result.custom_comparisons[comp_idx];
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
        .label(&plot.label)
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
