use crate::analysis_results::AnalysisResults;
use csv::Writer;
use std::path::{Path, PathBuf};

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
    "lenbits",
    "unique_values",
    "bit_order",
    "file_name",
];

pub fn write_field_csvs(
    results: &[AnalysisResults],
    output_dir: &Path,
    file_paths: &[PathBuf],
) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

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

    // Add group comparison CSVs
    let group_headers = &[
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

    if results.is_empty() {
        return Ok(());
    }

    // It's assumed all results correspond to same data/schema.
    for (comp_idx, comparison) in results[0].group_comparisons.iter().enumerate() {
        let mut wtr = Writer::from_path(
            output_dir.join(sanitize_filename(&comparison.name) + "_comparison.csv"),
        )?;
        wtr.write_record(group_headers)?;

        for (file_idx, result) in results.iter().enumerate() {
            // Get equivalent comparison for this result.
            let comparison = &result.group_comparisons[comp_idx];
            let base_group_lz: Vec<_> = comparison
                .group1_field_metrics
                .iter()
                .map(|m| m.lz_matches.to_string())
                .collect();
            let comp_group_lz: Vec<_> = comparison
                .group2_field_metrics
                .iter()
                .map(|m| m.lz_matches.to_string())
                .collect();
            let comp_group_entropy: Vec<_> = comparison
                .group2_field_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect();
            let base_group_entropy: Vec<_> = comparison
                .group1_field_metrics
                .iter()
                .map(|m| format!("{:.2}", m.entropy))
                .collect();

            let group2_lz_values: Vec<usize> = comparison
                .group2_field_metrics
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
                .group2_field_metrics
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

fn safe_ratio(child: usize, parent: usize) -> String {
    if parent == 0 {
        "0.0".into()
    } else {
        format!("{}", child as f64 / parent as f64)
    }
}

fn sanitize_filename(name: &str) -> String {
    name.replace(|c: char| !c.is_alphanumeric(), "_")
}
