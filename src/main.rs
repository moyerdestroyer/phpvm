mod cli;
mod config;
mod doctor;
mod install;
mod manifest;
mod matrix;
mod output;
mod providers;
mod runner;
mod version;

use anyhow::Result;
use clap::Parser;

use crate::cli::Command;

fn main() -> Result<()> {
    let args = cli::Args::parse();

    match args.command {
        Command::Install { version } => install::run(&version)?,
        Command::Run { version, command } => runner::run(&version, &command)?,
        Command::Matrix { command } => matrix::run(&command)?,
        Command::Doctor => doctor::run()?,
        Command::ReleaseCheck => doctor::release_check()?,
        Command::Versions => version::list_installed()?,
    }

    Ok(())
}
