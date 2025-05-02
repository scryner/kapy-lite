#![doc=include_str!("../README.md")]

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use config::Config;
use drive::auth::ListenPort;

mod cmd;
mod config;
mod drive;
mod gps;
mod image;
mod progress;

#[derive(Parser)]
#[command(
    author,
    version,
    about = "A copy utility for images with embedding GPS data",
    long_about = None
)]
struct Cli {
    /// Set a custom config file
    #[arg(short, long, value_name = "CONF_PATH", global = true)]
    config: Option<PathBuf>,

    /// Set google API credentials path
    #[arg(long, value_name = "CRED_PATH", global = true)]
    cred: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, PartialEq, Debug)]
enum Commands {
    /// Clone images according to policies defined at config file
    #[command(arg_required_else_help = false)]
    Clone {
        /// Set origin path to import
        #[arg(long, value_name = "FROM_PATH")]
        from: Option<PathBuf>,

        /// Set destination path to export
        #[arg(long, value_name = "TO_PATH")]
        to: Option<PathBuf>,

        /// Set ignore geotag
        #[arg(long, default_value_t = false)]
        ignore_geotag: bool,

        /// Show what would do without copying/writing to destination
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Import after specific date (YYYY-MM-DD or YYYY-MM or YYYY)
        #[arg(long, value_name = "AFTER")]
        after: Option<String>,
    },
    /// Initialize to make configuration file
    Init {
        /// Force overwritten
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Login to google drive
    Login {
        /// Listen port to exchange token for OAuth2.0
        #[arg(short, long)]
        listen_port: Option<i32>,
    },

    /// Clean credentials
    Clean,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Failure: {}", e);
        std::process::exit(1);
    }
}

async fn run(app: Cli) -> Result<()> {
    // do initialization if 'init' command
    if let Commands::Init { force } = app.command {
        return cmd::do_init(force).await;
    }

    // get default values
    let default_path = config::default_path();
    let default_config_path = default_path.config_path();
    let default_cred_path = default_path.cred_path();

    let conf_path = app
        .config
        .as_deref()
        .unwrap_or(default_config_path.as_ref());

    let mut conf = Config::build_from_file(conf_path).map_err(|e| {
        anyhow!(
            "Failed to build configuration: {}, you should run 'init' first",
            e
        )
    })?;

    let cred_path = app.cred.as_deref().unwrap_or(default_cred_path.as_ref());

    match &app.command {
        Commands::Clone {
            from,
            to,
            ignore_geotag,
            dry_run,
            after,
        } => {
            // override configuration with options
            if let Some(from) = from {
                conf.set_import_from(from.clone());
            }

            if let Some(to) = to {
                conf.set_import_to(to.clone());
            }

            cmd::do_clone(conf, cred_path, *ignore_geotag, *dry_run, after.clone()).await
        }
        Commands::Clean => cmd::do_clean(cred_path).await,
        Commands::Login { listen_port } => {
            let listen_port = match *listen_port {
                Some(port) => ListenPort::Port(port),
                None => ListenPort::DefaultPort,
            };

            cmd::do_login(cred_path, listen_port).await
        }
        _ => {
            // never reached
            Err(anyhow!("Unsupported command {:?}", &app.command))
        }
    }
}
