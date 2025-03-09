use argh::FromArgs;
use mimalloc::MiMalloc;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{
    fs::File,
    io::{stdout, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Instant,
};
use struct_compression_analyzer::{
    analyzer::{CompressionOptions, SchemaAnalyzer},
    brute_force::{
        brute_force_custom::CustomComparisonOptimizationResult,
        brute_force_split::SplitComparisonOptimizationResult, optimize_and_apply_coefficients,
        print_all_optimization_results,
    },
    csv,
    offset_evaluator::try_evaluate_file_offset,
    plot::generate_plots,
    results::{
        analysis_results::AnalysisResults, merged_analysis_results::MergedAnalysisResults,
        PrintFormat,
    },
    schema::Schema,
};
use walkdir::WalkDir;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[derive(Debug, FromArgs)]
/// CLI for analyzing struct compression
struct Args {
    #[argh(subcommand)]
    /// the command to execute.
    command: Command,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand)]
enum Command {
    File(FileCommand),
    Directory(DirectoryCommand),
}

#[derive(Debug, FromArgs)]
#[argh(subcommand, name = "analyze-file")]
/// Analyze a single file
struct FileCommand {
    #[argh(positional)]
    /// path to the schema file
    schema: PathBuf,

    #[argh(positional)]
    /// path to the file to analyze
    path: PathBuf,

    /// offset to start analyzing from
    #[argh(option, short = 'o')]
    offset: Option<u64>,

    /// length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    #[argh(option, short = 'l')]
    length: Option<u64>,

    /// output format ('detailed', 'concise')
    #[argh(option, short = 'f')]
    format: Option<PrintFormat>,

    /// show extra stats
    #[argh(switch, long = "show-extra-stats")]
    show_extra_stats: bool,

    /// zstd compression level (default: 3)
    #[argh(option, short = 'z', default = "3")]
    zstd_compression_level: i32,
}

#[derive(Debug, FromArgs)]
#[argh(subcommand, name = "analyze-directory")]
/// Analyze all files in a directory
struct DirectoryCommand {
    #[argh(positional)]
    /// path to the schema file
    schema: PathBuf,

    #[argh(positional)]
    /// path to the directory containing files to analyze
    path: PathBuf,

    /// offset to start analyzing from
    #[argh(option, short = 'o')]
    offset: Option<u64>,

    /// length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    #[argh(option, short = 'l')]
    length: Option<u64>,

    /// output format ('detailed', 'concise')
    #[argh(option, short = 'f')]
    format: Option<PrintFormat>,

    /// print info for all files
    #[argh(switch, short = 'a')]
    all_files: bool,

    /// output directory for CSV and plot reports
    #[argh(option)]
    output: Option<PathBuf>,

    /// show extra stats
    #[argh(switch, long = "show-extra-stats")]
    show_extra_stats: bool,

    /// zstd compression level (default: 16)
    #[argh(option, short = 'z', default = "16")]
    zstd_compression_level: i32,

    /// enable brute forcing of LZ match and entropy multiplier parameters
    #[argh(switch, long = "brute-force-lz-params")]
    brute_force: bool,
}

/// Parameters to function used to analyze a single file.
struct AnalyzeFileParams<'a> {
    /// The schema to use for analysis
    schema: &'a Schema,
    /// The path to the file being analyzed
    path: &'a PathBuf,
    /// The number of bytes per struct element
    bytes_per_element: u64,
    /// The offset to start analyzing from
    /// If not specified, we read based on schema, or assign 0.
    offset: Option<u64>,
    /// The length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    length: Option<u64>,
    /// The zstd compression level.
    zstd_compression_level: i32,
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    let start_time = Instant::now();
    match args.command {
        Command::File(file_cmd) => {
            let schema = load_schema(&file_cmd.schema)?;
            let analysis_result = analyze_file(&AnalyzeFileParams {
                schema: &schema,
                path: &file_cmd.path,
                bytes_per_element: (schema.root.bits / 8) as u64,
                offset: file_cmd.offset,
                length: file_cmd.length,
                zstd_compression_level: file_cmd.zstd_compression_level,
            })?;
            println!("Analysis Results:");
            analysis_result.print(
                &mut stdout(),
                &schema,
                file_cmd.format.unwrap_or(PrintFormat::default()),
                !file_cmd.show_extra_stats,
            )?;
        }
        Command::Directory(dir_cmd) => {
            let schema = load_schema(&dir_cmd.schema)?;
            let files = find_directory_files_recursive(&dir_cmd.path)?;
            println!(
                "Analyzing directory: {} ({} files)",
                dir_cmd.path.display(),
                files.len()
            );

            // Process every file with rayon, collecting individual results
            let analyze_start_time = Instant::now();
            let mut individual_results: Vec<AnalysisResults> = files
                .par_iter()
                .map(|path| {
                    analyze_file(&AnalyzeFileParams {
                        schema: &schema,
                        path,
                        bytes_per_element: (schema.root.bits / 8) as u64,
                        offset: dir_cmd.offset,
                        length: dir_cmd.length,
                        zstd_compression_level: dir_cmd.zstd_compression_level,
                    })
                })
                .filter_map(|result| match result {
                    Ok(results) => Some(results),
                    Err(e) => {
                        eprintln!("Error processing {}: {}", dir_cmd.path.display(), e);
                        None
                    }
                })
                .collect();

            // Run brute force optimization on merged results if enabled
            if dir_cmd.brute_force {
                println!("\nRunning LZ parameter optimization on merged results...");
                let brute_force_start_time = Instant::now();
                let (split_results, custom_results) =
                    optimize_and_apply_coefficients(&mut individual_results, None);
                println!(
                    "{}ms... Brute force optimization complete.",
                    brute_force_start_time.elapsed().as_millis()
                );

                print_all_optimization_results(&mut stdout(), &split_results, &custom_results)?;

                // Save optimization results to file if output directory is specified
                if let Some(output_dir) = &dir_cmd.output {
                    std::fs::create_dir_all(output_dir)?;
                    write_optimization_results_to_file(
                        &split_results,
                        &custom_results,
                        output_dir,
                    )?;
                }
            }

            // Merge all results
            println!(
                "{}ms... Merging {} files.",
                analyze_start_time.elapsed().as_millis(),
                individual_results.len()
            );
            let merge_start_time = Instant::now();
            let merged_results = MergedAnalysisResults::from_results(&individual_results)?;
            println!(
                "{}ms... Aggregated (Merged) Analysis Results:",
                merge_start_time.elapsed().as_millis()
            );

            merged_results.print(
                &mut stdout(),
                &schema,
                dir_cmd.format.unwrap_or(PrintFormat::default()),
                !dir_cmd.show_extra_stats,
            )?;

            // Print individual files
            if dir_cmd.all_files {
                println!("Individual Files:");
                for x in 0..individual_results.len() {
                    println!("- {}", files[x].display());
                    individual_results[x].print(
                        &mut stdout(),
                        &schema,
                        dir_cmd.format.unwrap_or(PrintFormat::default()),
                        !dir_cmd.show_extra_stats,
                    )?;
                    println!();
                }
            }

            // Write reports, output, etc.
            if let Some(output_dir) = &dir_cmd.output {
                std::fs::create_dir_all(output_dir)?;

                // Write analysis results to files
                write_merged_results_to_file(
                    &merged_results,
                    output_dir,
                    &schema,
                    dir_cmd.format.unwrap_or(PrintFormat::default()),
                    false,
                )?;
                write_individual_results_to_files(
                    &individual_results,
                    output_dir,
                    &schema,
                    dir_cmd.format.unwrap_or(PrintFormat::default()),
                    false,
                    &files,
                )?;

                // Write CSV reports
                csv::write_all_csvs(
                    &merged_results.original_results,
                    &merged_results,
                    output_dir,
                    &files,
                )?;
                generate_plots(&merged_results.original_results, output_dir).unwrap();
                println!("Generated reports in: {}", output_dir.display());
            }
        }
    }

    // Print time taken for analysis
    println!(
        "Analysis complete in {}ms",
        start_time.elapsed().as_millis()
    );

    Ok(())
}

fn analyze_file(params: &AnalyzeFileParams) -> anyhow::Result<AnalysisResults> {
    // Read the file contents
    let mut file = File::open(params.path)?;

    let offset = if params.offset.is_none() {
        try_evaluate_file_offset(&params.schema.conditional_offsets, &mut file)?.unwrap_or(0)
    } else {
        params.offset.unwrap_or(0)
    };

    // Read up to length in AnalyzeFileParams at file offset
    let length = match params.length {
        Some(l) => l,
        None => file.metadata()?.len() - offset,
    };

    file.seek(SeekFrom::Start(offset))?;
    let mut data = unsafe { Box::new_uninit_slice(length as usize).assume_init() };
    file.read_exact(&mut data)?;

    // Analyze the file with SchemaAnalyzer
    let mut analyzer = SchemaAnalyzer::new(
        params.schema,
        CompressionOptions::default().with_zstd_compression_level(params.zstd_compression_level),
    );
    let mut bytes_left = length;

    while bytes_left > 0 {
        let start_offset = length - bytes_left;
        let slice =
            &data[start_offset as usize..start_offset as usize + params.bytes_per_element as usize];
        analyzer.add_entry(slice)?;
        bytes_left -= params.bytes_per_element;
    }

    // Output the analysis results here
    Ok(analyzer.generate_results()?)
}

fn load_schema(schema_path: &Path) -> anyhow::Result<Schema> {
    Ok(Schema::load_from_file(schema_path)?)
}

fn find_directory_files_recursive(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let metadata = std::fs::metadata(entry.path())?;
        files.push((entry.path().to_path_buf(), metadata.len()));
    }

    // Sort by file size (descending),
    // this improves performance when processing files in parallel.
    files.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(files.into_iter().map(|(path, _)| path).collect())
}

/// Write merged analysis results to a file
fn write_merged_results_to_file(
    merged_results: &MergedAnalysisResults,
    output_dir: &Path,
    schema: &Schema,
    format: PrintFormat,
    skip_misc_stats: bool,
) -> std::io::Result<()> {
    let output_path = output_dir.join("overall-result.txt");
    let mut file = File::create(output_path)?;
    merged_results.print(&mut file, schema, format, skip_misc_stats)?;
    Ok(())
}

/// Write individual analysis results to files in a subdirectory
fn write_individual_results_to_files(
    individual_results: &[AnalysisResults],
    output_dir: &Path,
    schema: &Schema,
    format: PrintFormat,
    skip_misc_stats: bool,
    files: &[PathBuf],
) -> std::io::Result<()> {
    // Create analysis_results directory
    let results_dir = output_dir.join("analysis_results");
    std::fs::create_dir_all(&results_dir)?;

    // Write each file's results
    for (x, result) in individual_results.iter().enumerate() {
        let file_name = files[x]
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .map(|s| format!("{}.txt", s))
            .unwrap_or_else(|| format!("result_{}.txt", x));

        let output_path = results_dir.join(file_name);
        let mut file = File::create(output_path)?;
        result.print(&mut file, schema, format, skip_misc_stats)?;
    }

    Ok(())
}

/// Write optimization results to a file
fn write_optimization_results_to_file(
    split_results: &[(String, SplitComparisonOptimizationResult)],
    custom_results: &[(String, CustomComparisonOptimizationResult)],
    output_dir: &Path,
) -> std::io::Result<()> {
    let output_path = output_dir.join("brute-force-results.txt");
    let mut file = File::create(output_path)?;
    print_all_optimization_results(&mut file, split_results, custom_results)?;
    Ok(())
}
