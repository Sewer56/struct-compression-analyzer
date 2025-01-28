use argh::FromArgs;
use std::path::PathBuf;

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
    bytes: Option<usize>,

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
    bytes: Option<usize>,

    /// offset to start analyzing from
    #[argh(option, short = 'o')]
    offset: Option<usize>,

    /// length of the data to analyze. If not specified, the entire rest of the file is analyzed.
    #[argh(option, short = 'l')]
    length: Option<usize>,
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    match args.command {
        Command::File(file_cmd) => {
            println!("Analysis complete for: {}", file_cmd.path.display());
        }
        Command::Directory(dir_cmd) => {
            println!("Analyzing directory: {}", dir_cmd.path.display());
            // Add code to analyze all files in the directory here
        }
    }

    Ok(())
}
