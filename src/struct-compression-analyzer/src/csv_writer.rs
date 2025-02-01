use crate::analysis_results::{get_parent_path, AnalysisResults};
use csv::Writer;
use std::{collections::HashMap, path::Path};

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
];

const CSV_SPLIT_HEADERS: &[&str] = &[
    "name",
    "full_path",
    "depth",
    "entropy",
    "estimated_size",
    "zstd_size",
    "original_size",
    "children_estimated_size",
    "children_zstd_size",
    "children_original_size",
    "children_estimated_ratio",
    "children_zstd_ratio",
    "children_original_ratio",
];

pub fn write_field_csvs(results: &[AnalysisResults], output_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    // Get field paths from first result (all results have same fields)
    let field_paths = results[0].per_field.keys();
    for field_path in field_paths {
        let mut wtr = Writer::from_path(output_dir.join(sanitize_filename(field_path) + ".csv"))?;
        wtr.write_record(CSV_HEADERS)?;

        // Write all individual field and group records
        for result in results {
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
        wtr.write_record(CSV_SPLIT_HEADERS)?;

        for result in results {
            // Sum up the estimates, zstd sizes, original sizes
            let mut children_estimated_size = 0;
            let mut children_zstd_size = 0;
            let mut children_original_size = 0;
            for child in &children {
                if let Some(field) = result.per_field.get(*child) {
                    children_estimated_size += field.estimated_size;
                    children_zstd_size += field.zstd_size;
                    children_original_size += field.original_size;
                }
            }

            // Write split record.
            if let Some(parent_field) = result.per_field.get(parent_path) {
                wtr.write_record(vec![
                    parent_field.name.clone(),
                    parent_field.full_path.clone(),
                    parent_field.depth.to_string(),
                    parent_field.entropy.to_string(),
                    parent_field.estimated_size.to_string(),
                    parent_field.zstd_size.to_string(),
                    parent_field.original_size.to_string(),
                    children_estimated_size.to_string(),
                    children_zstd_size.to_string(),
                    children_original_size.to_string(),
                    safe_ratio(children_estimated_size, parent_field.estimated_size),
                    safe_ratio(children_zstd_size, parent_field.zstd_size),
                    safe_ratio(children_original_size, parent_field.original_size),
                ])?;
            }
        }

        wtr.flush()?;
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
