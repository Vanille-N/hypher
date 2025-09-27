use std::path::{Path, PathBuf};
use std::fs;
use clap::{Parser, Subcommand};
use std::error::Error;

#[derive(Parser)]
#[clap(name = "hypher", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Builds a trie from a pattern file.
    Build {
        /// Input file to read the patterns from.
        file: PathBuf,
        /// Destination file to write the trie to.
        dest: PathBuf,
    },
    /// Splits a word into syllables.
    Query {
        /// Optional language to use.
        /// If this is not specified, then `--trie` MUST be given instead.
        #[arg(long, value_name="ISO")]
        lang: Option<String>,
        /// Optional pattern file to use.
        /// If this is not specifed, then `--lang` MUST be given instead.
        #[arg(long, value_name="BIN")]
        trie: Option<PathBuf>,
        /// Word to segment into syllables.
        word: String,
    },
}

fn build_trie(source: &Path, dest: &Path) -> Result<(), Box<dyn Error>> {
    let trie = hypher::builder::build_trie(source);
    fs::write(dest, &trie)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match &cli.command {
        Some(Command::Build { file, dest }) => {
            build_trie(file, dest)
        },
        Some(Command::Query { lang: code, trie, word }) => {
            match (code, trie) {
                (Some(code), None) => {
                    if code.len() != 2 {
                        return Err(format!("--lang={} is not a valid ISO code.", code).into())
                    }
                    let bytes = code.as_bytes();
                    let lang = hypher::Lang::from_iso([bytes[0], bytes[1]]).ok_or_else(|| {
                        format!("--lang={} is not a valid ISO code.", code)
                    })?;
                    let ans = hypher::hyphenate(word, lang).join("-");
                    println!("{}", ans);
                    Ok(())
                },
                (None, Some(file)) => {
                    let trie_data = fs::read(file)?;
                    let lang = hypher::Lang::from_bytes(
                        (1, 2), // TODO: what should I pick here?
                        &trie_data,
                    );
                    let ans = hypher::hyphenate(word, lang).join("-");
                    println!("{}", ans);
                    Ok(())
                }
                (None, None) | (Some(_), Some(_)) => {
                    Err(format!("must specify exactly one of `--lang` or `--trie`").into())
                }
            }
        }
        None => Ok(()),
    }
}
