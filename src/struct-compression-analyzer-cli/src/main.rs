use argh::FromArgs;
use std::{
    fs::{read_dir, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Instant,
};
use struct_compression_analyzer::{
    analysis_results::AnalysisResults, analyzer::SchemaAnalyzer,
    offset_evaluator::try_evaluate_file_offset, schema::Schema,
};

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
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    match args.command {
        Command::File(file_cmd) => {
            // Start stopwatch
            let start_time = Instant::now();
            let schema = load_schema(&file_cmd.schema)?;
            let analysis_result = analyze_file(&AnalyzeFileParams {
                schema: &schema,
                path: &file_cmd.path,
                bytes_per_element: (schema.root.bits / 8) as u64,
                offset: file_cmd.offset,
                length: file_cmd.length,
            })?;
            analysis_result.print();

            // Print time taken for analysis
            println!(
                "Analysis complete for: {} in {}ms",
                file_cmd.path.display(),
                start_time.elapsed().as_millis()
            );
        }
        Command::Directory(dir_cmd) => {
            println!("Analyzing directory: {}", dir_cmd.path.display());

            let schema = load_schema(&dir_cmd.schema)?;
            let analysis_result: Option<AnalysisResults> = None;
            let mut files = Vec::new();

            for file in read_dir(dir_cmd.path)? {
                let file = file?;
                let path = file.path();
                println!("Found file: {}", path.display());
                files.push(path);
            }

            // Load schema from JSON file

            // Process every file with rayon, outputting AnalysisResults
            // Using parallel iterator
            println!("Finished processing all files.");

            // Add code to analyze all files in the directory here
        }
    }

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
    let mut analyzer = SchemaAnalyzer::new(params.schema);
    let mut bytes_left = length;

    while bytes_left > 0 {
        let start_offset = length - bytes_left;
        let slice =
            &data[start_offset as usize..start_offset as usize + params.bytes_per_element as usize];
        analyzer.add_entry(slice);
        bytes_left -= params.bytes_per_element;
    }

    // Output the analysis results here
    Ok(analyzer.generate_results())
}

fn load_schema(schema_path: &Path) -> anyhow::Result<Schema> {
    Ok(Schema::load_from_file(schema_path)?)
}
