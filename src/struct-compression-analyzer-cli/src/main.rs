// main.rs
use argh::FromArgs;

#[derive(FromArgs, PartialEq, Debug)]
/// My awesome program
struct Cli {
    #[argh(subcommand)]
    command: Command,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Command {
    /// The existing subcommand
    AnalyzeFile(AnalyzeDirectory),

    /// The new subcommand
    AnalyzeFolder(AnalyzeFile),
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "analyze-directory")]
/// Analyzes a given directory and prints results
struct AnalyzeDirectory {
    /// path to the directory to analyze
    #[argh(positional)]
    dir_path: String,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "analyze-file")]
/// Analyzes a given file and prints results
struct AnalyzeFile {
    /// path to the file to analyze
    #[argh(positional)]
    file_path: String,
}

fn main() {
    let cli: Cli = argh::from_env();
    match cli.command {
        Command::AnalyzeFile(subcommand) => existing_subcommand(subcommand),
        Command::AnalyzeFolder(subcommand) => new_subcommand(subcommand),
    }
}

fn existing_subcommand(subcommand: AnalyzeDirectory) {
    println!(
        "Handling existing subcommand with arg: {}",
        subcommand.dir_path
    );
}

fn new_subcommand(subcommand: AnalyzeFile) {
    println!("Handling new subcommand with arg: {}", subcommand.file_path);
}
