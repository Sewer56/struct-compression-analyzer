use crate::analysis_results::AnalysisResults;
use csv::Writer;
use std::path::{Path, PathBuf};

/// Writes all CSVs related to analysis results.
///
/// This function orchestrates the writing of multiple CSV files:
/// - Per-field statistics.
/// - Group comparison statistics.
/// - Per-field value statistics.
/// - Per-field bit statistics.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `merged_results` -  An [`AnalysisResults`] object representing the merged results of all files.
/// * `output_dir` - The directory where the CSV files will be written.
/// * `file_paths` - A slice of [`PathBuf`]s representing the original file paths for each result.
///
/// # Returns
///
/// * `std::io::Result<()>` -  Ok if successful, otherwise an error.
pub fn write_all_csvs(
    results: &[AnalysisResults],
    merged_results: &AnalysisResults,
    output_dir: &Path,
    file_paths: &[PathBuf],
) -> std::io::Result<()> {
    write_field_csvs(results, output_dir, file_paths)?;
    write_group_comparison_csv(results, output_dir, file_paths)?;
    write_field_value_stats_csv(merged_results, output_dir)?;
    write_field_bit_stats_csv(merged_results, output_dir)?;
    Ok(())
}

/// Writes individual CSV files for each field, containing statistics across all input files.
///
/// Creates one CSV file per field. Each row in a field's CSV represents the
/// field's metrics from one of the input files.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `merged_results` - An [`AnalysisResults`] object representing the merged results.
/// * `output_dir` - The directory where the CSV files will be written.
/// * `file_paths` - A slice of [`PathBuf`]s representing the original file paths for each result.
///
/// # Returns
///
/// * `std::io::Result<()>` - Ok if successful, otherwise an error.
pub fn write_field_csvs(
    results: &[AnalysisResults],
    output_dir: &Path,
    file_paths: &[PathBuf],
) -> std::io::Result<()> {
    const CSV_HEADERS: &[&str] = &[
        "name",
        "full_path",
        "depth",
        "entropy",
        "lz_matches",
        "lz_matches_pct",
        "estimated_size",
        "zstd_size",
        "original_size",
        "estimated_size_pct",
        "zstd_size_pct",
        "original_size_pct",
        "zstd_ratio",
        "lenbits",
        "unique_values",
        "bit_order",
        "file_name",
    ];

    // Get field paths from first result (all results have same fields)
    let field_paths = results[0].per_field.keys();
    for field_path in field_paths {
        let mut wtr = Writer::from_path(output_dir.join(sanitize_filename(field_path) + ".csv"))?;
        wtr.write_record(CSV_HEADERS)?;

        // Write all individual field and group records
        for x in 0..results.len() {
            let result = &results[x];
            let file_path = &file_paths[x];
            let file_metrics = result.as_field_metrics();
            if let Some(field) = result.per_field.get(field_path) {
                let parent_stats = field.parent_metrics_or(result, &file_metrics);
                wtr.write_record(vec![
                    field.name.clone(),
                    field.full_path.clone(),
                    field.depth.to_string(),
                    field.entropy.to_string(),
                    field.lz_matches.to_string(),
                    safe_ratio(field.lz_matches, parent_stats.lz_matches),
                    field.estimated_size.to_string(),
                    field.zstd_size.to_string(),
                    field.original_size.to_string(),
                    safe_ratio(field.estimated_size, parent_stats.estimated_size),
                    safe_ratio(field.zstd_size, parent_stats.zstd_size),
                    safe_ratio(field.original_size, parent_stats.original_size),
                    safe_ratio(field.zstd_size, field.original_size),
                    field.lenbits.to_string(),
                    field.value_counts.len().to_string(),
                    format!("{:?}", field.bit_order),
                    file_path
                        .file_name()
                        .and_then(|os_str| os_str.to_str())
                        .unwrap_or_default()
                        .to_string(),
                ])?;
            }
        }
        wtr.flush()?;
    }

    Ok(())
}

/// Writes CSV files comparing groups of fields within each file.
///
/// This function generates CSV files that compare two groups of fields
/// (defined in the schema) within each analyzed file.  It reports on
/// differences in size, LZ77 matches, estimated size, and Zstd compression.
///
/// # Arguments
///
/// * `results` - A slice of [`AnalysisResults`], one for each analyzed file.
/// * `merged_results` - An [`AnalysisResults`] object representing the merged results of all files.
/// * `output_dir` - The directory where the CSV files will be written.
/// * `file_paths` - A slice of `PathBuf`s representing the original file paths for each result.
///
/// # Returns
///
/// * `std::io::Result<()>` - Ok if successful, otherwise an error.
pub fn write_group_comparison_csv(
    results: &[AnalysisResults],
    output_dir: &Path,
    file_paths: &[PathBuf],
) -> std::io::Result<()> {
    // Add group comparison CSVs
    const GROUP_HEADERS: &[&str] = &[
        "name",
        "file_name",
        "size",
        "base lz",
        "comp lz",
        "base est",
        "base zstd",
        "comp est",
        "comp zstd",
        "ratio est",
        "ratio zstd",
        "diff est",
        "diff zstd",
        "base group lz",
        "comp group lz",
        "base group entropy",
        "comp group entropy",
        "max comp lz diff",
        "max comp entropy diff",
    ];

    for (comp_idx, comparison) in results[0].split_comparisons.iter().enumerate() {
        let mut wtr = Writer::from_path(
            output_dir.join(sanitize_filename(&comparison.name) + "_comparison.csv"),
        )?;
        wtr.write_record(GROUP_HEADERS)?;

        for (file_idx, result) in results.iter().enumerate() {
            // Get equivalent comparison for this result.
            let comparison = &result.split_comparisons[comp_idx];
            let base_group_lz: Vec<_> = comparison
                .baseline_comparison_metrics
                .iter()
                .map(|m| m.lz_matches.to_string())
                .collect();
            let comp_group_lz: Vec<_> = comparison
                .split_comparison_metrics
                .iter()
                .map(|m| m.lz_matches.to_string())
                .collect();
            let comp_group_entropy: Vec<_> = comparison
                .split_comparison_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect();
            let base_group_entropy: Vec<_> = comparison
                .baseline_comparison_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect();

            let group2_lz_values: Vec<usize> = comparison
                .split_comparison_metrics
                .iter()
                .map(|m| m.lz_matches)
                .collect();

            let max_intra_comp_lz_diff_ratio = if group2_lz_values.len() < 2 {
                0.0
            } else {
                let max = *group2_lz_values.iter().max().unwrap() as f64;
                let min = *group2_lz_values.iter().min().unwrap() as f64;
                max / min
            };

            let group2_entropy_values: Vec<f64> = comparison
                .split_comparison_metrics
                .iter()
                .map(|m| m.entropy)
                .collect();

            let max_intra_comp_entropy_diff = if group2_entropy_values.len() < 2 {
                0.0
            } else {
                let max = group2_entropy_values
                    .iter()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap();
                let min = group2_entropy_values
                    .iter()
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap();
                max - min
            };

            wtr.write_record(vec![
                comparison.name.clone(), // name
                file_paths[file_idx]
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap(), // file name
                comparison.group1_metrics.original_size.to_string(), // size
                comparison.group1_metrics.lz_matches.to_string(), // base lz
                comparison.group2_metrics.lz_matches.to_string(), // comp lz
                comparison.group1_metrics.estimated_size.to_string(), // base est
                comparison.group1_metrics.zstd_size.to_string(), // base zstd
                comparison.group2_metrics.estimated_size.to_string(), // comp est
                comparison.group2_metrics.zstd_size.to_string(), // comp zstd
                safe_ratio(
                    comparison.group2_metrics.estimated_size as usize,
                    comparison.group1_metrics.estimated_size as usize,
                ), // ratio est
                safe_ratio(
                    comparison.group2_metrics.zstd_size as usize,
                    comparison.group1_metrics.zstd_size as usize,
                ), // ratio zstd
                comparison.difference.estimated_size.to_string(), // diff est
                comparison.difference.zstd_size.to_string(), // diff zstd
                base_group_lz.join("|"),
                comp_group_lz.join("|"),
                base_group_entropy.join("|"),
                comp_group_entropy.join("|"),
                format!("{:.2}", max_intra_comp_lz_diff_ratio),
                format!("{:.2}", max_intra_comp_entropy_diff),
            ])?;

            wtr.flush()?;
        }
    }

    Ok(())
}

/// Writes CSV files containing value statistics for each field.
///
/// This function generates a CSV file for each field, listing the unique values
/// encountered in the merged data, along with their counts and ratios.
///
/// # Arguments
///
/// * `results` - The merged `AnalysisResults` object.
/// * `output_dir` - The directory where the CSV files will be written.
///
/// # Returns
///
/// * `std::io::Result<()>` - Ok if successful, otherwise an error.
pub fn write_field_value_stats_csv(
    results: &AnalysisResults,
    output_dir: &Path,
) -> std::io::Result<()> {
    // Get field paths from first result
    let field_paths = results.per_field.keys();
    for field_path in field_paths {
        let mut wtr =
            Writer::from_path(output_dir.join(sanitize_filename(field_path) + "_value_stats.csv"))?;
        wtr.write_record(["value", "count", "ratio"])?;

        // Write value counts for each result
        if let Some(field) = results.per_field.get(field_path) {
            // Get sorted value counts
            let value_counts = field.sorted_value_counts();

            // Calculate total count for ratio
            let total_values: usize = value_counts.iter().map(|(_, count)| **count as usize).sum();

            // Write sorted values with ratios
            for (value, count) in value_counts {
                wtr.write_record(&[
                    value.to_string(),
                    count.to_string(),
                    safe_ratio(*count as usize, total_values),
                ])?;
            }
        }
        wtr.flush()?;
    }
    Ok(())
}

/// Writes CSV files containing bit-level statistics for each field.
///
/// This function generates a CSV file for each field, showing the counts of 0s
/// and 1s at each bit offset within the field, along with the ratio of 0s to
/// the total number of bits at that offset.
///
/// # Arguments
///
/// * `results` - The merged `AnalysisResults` object.
/// * `output_dir` - The directory where the CSV files will be written.
///
/// # Returns
///
/// * `std::io::Result<()>` - Ok if successful, otherwise an error.
pub fn write_field_bit_stats_csv(
    results: &AnalysisResults,
    output_dir: &Path,
) -> std::io::Result<()> {
    // Get field paths from first result
    let field_paths = results.per_field.keys();
    for field_path in field_paths {
        let mut wtr =
            Writer::from_path(output_dir.join(sanitize_filename(field_path) + "_bit_stats.csv"))?;
        wtr.write_record(["bit_offset", "zero_count", "one_count", "ratio"])?;

        // Write bit stats for each result
        if let Some(field) = results.per_field.get(field_path) {
            for (i, stats) in field.bit_counts.iter().enumerate() {
                wtr.write_record(&[
                    i.to_string(),
                    stats.zeros.to_string(),
                    stats.ones.to_string(),
                    safe_ratio(
                        stats.zeros as usize,
                        stats.zeros as usize + stats.ones as usize,
                    ),
                ])?;
            }
        }
        wtr.flush()?;
    }
    Ok(())
}

/// Calculates a safe ratio between two numbers, handling division by zero.
///
/// # Arguments
///
/// * `child` - The numerator.
/// * `parent` - The denominator.
///
/// # Returns
///
/// A string representing the ratio, or "0.0" if the denominator is zero.
fn safe_ratio(child: usize, parent: usize) -> String {
    if parent == 0 {
        "0.0".into()
    } else {
        format!("{}", child as f64 / parent as f64)
    }
}

/// Sanitizes a string to be used as a filename by replacing non-alphanumeric characters with underscores.
/// # Arguments
///
/// * `name` - The input string.
///
/// # Returns
/// A sanitized version of the string suitable for use as a filename.
fn sanitize_filename(name: &str) -> String {
    name.replace(|c: char| !c.is_alphanumeric(), "_")
}
