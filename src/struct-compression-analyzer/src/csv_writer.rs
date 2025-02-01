use crate::analysis_results::{get_parent_path, AnalysisResults};
use csv::Writer;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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

const CSV_SPLIT_HEADERS: &[&str] = &[
    "file_name",
    "parent_full_path",
    "parent_entropy",
    "parent_lz_matches",
    "parent_estimated_size",
    "parent_zstd_size",
    "parent_original_size",
    "child_estimated_size",
    "child_zstd_size",
    "child_estimated_ratio",
    "child_zstd_ratio",
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

    // Calculate all parent->child mappings for calculating splits.
    let mut parent_to_child: HashMap<&str, Vec<&str>> = HashMap::new();
    let field_paths = results[0].per_field.keys();
    for field_path in field_paths {
        let parent_path = get_parent_path(field_path).unwrap_or("");
        // Append to parent->child mapping
        if !parent_to_child.contains_key(parent_path) {
            parent_to_child.insert(parent_path, vec![field_path]);
        } else {
            parent_to_child
                .get_mut(parent_path)
                .unwrap()
                .push(field_path);
        }
    }

    // Write all of the split data
    for (parent_path, children) in parent_to_child {
        let mut wtr = Writer::from_path(
            output_dir.join(format!("{}_split.csv", sanitize_filename(parent_path))),
        )?;

        // Generate all columns.
        let mut child_columns: Vec<String> = Vec::new();
        for child in &children {
            child_columns.push(format!("{}_entropy", get_child_field_name(child)));
            child_columns.push(format!("{}_lz_matches", get_child_field_name(child)));
            child_columns.push(format!("{}_estimated_size", get_child_field_name(child)));
            child_columns.push(format!("{}_zstd_size", get_child_field_name(child)));
            child_columns.push(format!("{}_original_size", get_child_field_name(child)));
        }

        // Write header with all columns
        let mut all_columns = CSV_SPLIT_HEADERS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        all_columns.extend_from_slice(&child_columns);
        wtr.write_record(&all_columns)?;

        for x in 0..results.len() {
            let result = &results[x];
            let file_path = &file_paths[x];

            // Sum up the estimates, zstd sizes, original sizes
            let mut children_estimated_size = 0;
            let mut children_zstd_size = 0;
            for child in &children {
                if let Some(field) = result.per_field.get(*child) {
                    children_estimated_size += field.estimated_size;
                    children_zstd_size += field.zstd_size;
                }
            }

            // Write split record.
            if let Some(parent_field) = result.per_field.get(parent_path) {
                let mut record = vec![
                    file_path
                        .file_name()
                        .and_then(|os_str| os_str.to_str())
                        .unwrap_or_default()
                        .to_string(),
                    parent_field.full_path.clone(), // parent_full_path
                    parent_field.entropy.to_string(), // parent_entropy
                    parent_field.lz_matches.to_string(), // parent_lz_matches
                    parent_field.estimated_size.to_string(), // parent_estimated_size
                    parent_field.zstd_size.to_string(), // parent_zstd_size
                    parent_field.original_size.to_string(), // parent_original_size
                    children_estimated_size.to_string(), // child_estimated_size
                    children_zstd_size.to_string(), // child_zstd_size
                    safe_ratio(children_estimated_size, parent_field.estimated_size), // child_estimated_ratio
                    safe_ratio(children_zstd_size, parent_field.zstd_size), // child_zstd_ratio
                ];

                // Now write child specific fields to the record
                for child in &children {
                    if let Some(child_field) = result.per_field.get(*child) {
                        record.extend_from_slice(&[
                            child_field.entropy.to_string(),        // "{}_entropy"
                            child_field.lz_matches.to_string(),     // "{}_lz_matches"
                            child_field.estimated_size.to_string(), // "{}_estimated_size"
                            child_field.zstd_size.to_string(),      // "{}_zstd_size"
                            child_field.original_size.to_string(),  // "{}_original_size"
                        ]);
                    }
                }
                wtr.write_record(&record)?;
            }
        }

        wtr.flush()?;
    }

    Ok(())
}

fn get_child_field_name(field_path: &str) -> String {
    field_path
        .split('.')
        .next_back()
        .unwrap_or_default()
        .to_string()
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
