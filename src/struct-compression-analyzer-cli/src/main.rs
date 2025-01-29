use argh::FromArgs;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{
    fs::{read_dir, File},
    io::{Read, Seek},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    time::Instant,
};
use struct_compression_analyzer::{
    analysis_results::AnalysisResults, analyzer::SchemaAnalyzer, schema::Schema,
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

    #[argh(option, short = 'b')]
    /// number of bytes per struct element
    bytes_per_element: usize,

    /// offset to start analyzing from
    #[argh(option, short = 'o')]
    offset: Option<usize>,

    /// length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    #[argh(option, short = 'l')]
    length: Option<usize>,
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

    #[argh(option, short = 'b')]
    /// number of bytes per struct element
    bytes_by_element: usize,

    /// offset to start analyzing from
    #[argh(option, short = 'o')]
    offset: Option<usize>,

    /// length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    #[argh(option, short = 'l')]
    length: Option<usize>,
}

/// Parameters to function used to analyze a single file.
struct AnalyzeFileParams<'a> {
    // The schema to use for analysis
    schema: &'a Schema,
    // The path to the file being analyzed
    path: &'a PathBuf,
    // The number of bytes per struct element
    bytes_per_element: usize,
    // The offset to start analyzing from
    offset: usize,
    // The length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    length: Option<usize>,
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
                bytes_per_element: file_cmd.bytes_per_element,
                offset: file_cmd.offset.unwrap_or(0),
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

            let schema = load_schema(&dir_cmd.schema);
            let analysis_result: Option<AnalysisResults> = None;
            let mut files = Vec::new();

            for file in read_dir(dir_cmd.path)? {
                let file = file?;
                let path = file.path();
                println!("Found file: {}", path.display());
                files.push(path);
            }

            // Process every file with rayon, outputting AnalysisResults
            // Using parallel iterator
            //files.par_iter()
            //    .for_each(|input| );

            // Add code to analyze all files in the directory here
        }
    }

    Ok(())
}

fn analyze_file(params: &AnalyzeFileParams) -> anyhow::Result<AnalysisResults> {
    // Read the file contents
    let file = File::open(params.path)?;

    // Read up to length in AnalyzeFileParams at file offset
    let length = match params.length {
        Some(l) => l,
        None => file.metadata()?.len() as usize - params.offset,
    };

    let mut data = unsafe { Box::new_uninit_slice(length).assume_init() };
    file.read_exact_at(&mut data, params.offset as u64)?;

    // Analyze the file with SchemaAnalyzer
    let mut analyzer = SchemaAnalyzer::new(params.schema);
    let mut bytes_left = length;

    while bytes_left > 0 {
        let start_offset = length - bytes_left;
        let slice = &data[start_offset..start_offset + params.bytes_per_element];
        analyzer.add_entry(slice);
        bytes_left -= params.bytes_per_element;
    }

    // Output the analysis results here
    Ok(analyzer.generate_results())
}

fn load_schema(schema_path: &Path) -> anyhow::Result<Schema> {
    Ok(Schema::load_from_file(schema_path)?)
}
