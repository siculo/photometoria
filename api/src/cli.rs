// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = "config.toml";

/// Command-line arguments.
#[derive(Debug)]
pub struct Args {
    /// Path to configuration file.
    pub config: PathBuf,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            config: PathBuf::from(DEFAULT_CONFIG_PATH),
        }
    }
}

/// Parses command-line arguments.
///
/// Supports:
/// - `-c <path>` or `--config <path>`: specify configuration file path
/// - `-h` or `--help`: show usage information
///
/// Returns default arguments if no options are provided.
pub fn parse_args() -> Args {
    let mut args = Args::default();
    let mut iter = std::env::args().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-c" | "--config" => {
                let path = iter.next().unwrap_or_else(|| {
                    eprintln!("Error: --config requires a path argument");
                    std::process::exit(1);
                });
                args.config = PathBuf::from(path);
            }
            other => {
                eprintln!("Error: unknown argument '{}'", other);
                eprintln!("Use --help for usage information");
                std::process::exit(1);
            }
        }
    }

    args
}

fn print_help() {
    let binary_name = std::env::args()
        .next()
        .unwrap_or_else(|| "photometoria-rest-api".to_string());

    println!(
        "Photometoria REST API Server

USAGE:
    {} [OPTIONS]

OPTIONS:
    -c, --config <PATH>    Path to configuration file [default: config.toml]
    -h, --help             Print this help message
",
        binary_name
    );
}
