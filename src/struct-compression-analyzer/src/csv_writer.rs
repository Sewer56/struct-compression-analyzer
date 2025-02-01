use crate::analysis_results::AnalysisResults;
use csv::Writer;
use std::path::Path;

const CSV_HEADERS: &[&str] = &[
    "name",
    "full_path",
    "depth",
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

pub fn write_field_csvs(results: &[AnalysisResults], output_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    // Get field paths from first result (all results have same fields)
    let field_paths = results[0].per_field.keys();

    for field_path in field_paths {
        let mut wtr = Writer::from_path(output_dir.join(sanitize_filename(field_path) + ".csv"))?;
        wtr.write_record(CSV_HEADERS)?;

        for result in results {
            let file_metrics = result.as_field_metrics();
            if let Some(field) = result.per_field.get(field_path) {
                let parent_stats = field.parent_metrics_or(result, &file_metrics);
                wtr.write_record(vec![
                    field.name.clone(),
                    field.full_path.clone(),
                    field.depth.to_string(),
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
