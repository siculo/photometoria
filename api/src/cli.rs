// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Photometoria REST API Server
#[derive(Parser)]
#[command(name = "photometoria", version)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short = 'c', long, default_value = "config.toml", global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Configuration management
    Config {
        #[command(subcommand)]
        subcommand: ConfigCommand,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Validate the configuration file without starting the server
    Check,
    /// Print the effective configuration as valid TOML
    Show,
    /// Print the value of a single configuration key
    Get {
        /// Dotted key path (e.g. server.port, storage.path)
        key: String,
    },
}
