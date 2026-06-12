mod cli;
mod config;
mod doctor;
mod install;
mod manifest;
mod matrix;
mod output;
mod profile;
mod providers;
mod runner;
mod version;

use anyhow::Result;
use clap::Parser;

use crate::cli::Command;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    let format = args.command.output_format();

    match args.command {
        Command::Install { version, profile } => install::run(&version, profile.as_deref())?,
        Command::Run { version, command } => runner::run(&version, &command)?,
        Command::Matrix { command, .. } => matrix::run_with_format(&command, format)?,
        Command::Doctor { .. } => doctor::run_with_format(format)?,
        Command::ReleaseCheck { .. } => doctor::release_check_with_format(format)?,
        Command::Profiles { .. } => profile::list_profiles(format)?,
        Command::Versions => version::list_installed()?,
    }

    Ok(())
}
