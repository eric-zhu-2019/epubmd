use clap::Parser;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[command(name = "epubmd", version = VERSION, about = "Convert a DRM-free EPUB into a Markdown zip archive.")]
struct Cli {
    /// Input EPUB path.
    input: PathBuf,

    /// Destination .zip path. Defaults to <book>.zip next to the EPUB.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Replace an existing destination zip.
    #[arg(short = 'f', long = "force")]
    force: bool,
}

fn main() {
    let cli = Cli::parse();
    let output = cli
        .output
        .unwrap_or_else(|| cli.input.with_extension("zip"));
    match epubmd_core::convert_epub_to_zip(&cli.input, &output, cli.force) {
        Ok(path) => println!("Wrote {}", path.display()),
        Err(error) => {
            eprintln!("epubmd: {error}");
            std::process::exit(1);
        }
    }
}
