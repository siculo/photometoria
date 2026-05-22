// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

mod cli;

use std::path::Path;

use clap::Parser;
use photometoria_rest_api::config::load_config;
use photometoria_rest_api::routes;
use photometoria_rest_api::startup::{init_app_state, init_tracing, shutdown_signal};

use cli::{Cli, Command, ConfigCommand};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Config { subcommand }) => match subcommand {
            ConfigCommand::Check => config_check(&cli.config),
            ConfigCommand::Show => config_show(&cli.config),
            ConfigCommand::Get { key } => config_get(&cli.config, &key),
        },
        None => run_server(&cli.config).await,
    }
}

fn config_check(config_path: &Path) {
    match load_config(config_path) {
        Ok(_) => println!("Configuration is valid."),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn config_get(config_path: &Path, key: &str) {
    let config = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };
    match config.get_value(key) {
        Ok(value) => print!("{value}"),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

fn config_show(config_path: &Path) {
    let config = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };
    match toml::to_string(&config) {
        Ok(output) => print!("{output}"),
        Err(e) => {
            eprintln!("Error serializing configuration: {e}");
            std::process::exit(1);
        }
    }
}

async fn run_server(config_path: &Path) {
    init_tracing();

    tracing::info!("Starting Photometoria REST API...");

    let config = match load_config(config_path) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };

    let addr = config.server_addr();
    tracing::info!("Server listening on http://{addr}");

    let state = match init_app_state(config).await {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
    };
    let app = routes::create_router(state.clone());

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind to {addr}: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!("Server error: {e}");
        std::process::exit(1);
    }

    state.worker_pool.lock().await.shutdown().await;

    tracing::info!("Server stopped");
}
