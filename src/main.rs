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

    match args.command {
        Command::Install { version, profile } => install::run(&version, profile.as_deref())?,
        Command::Run { version, command } => runner::run(&version, &command)?,
        Command::Matrix { report, command } => {
            let format = cli::parse_report_format(&report);
            matrix::run_with_format(&command, format)?
        }
        Command::Doctor { json } => {
            let format = if json {
                output::OutputFormat::Json
            } else {
                output::OutputFormat::Human
            };
            doctor::run_with_format(format)?
        }
        Command::ReleaseCheck { json } => {
            let format = if json {
                output::OutputFormat::Json
            } else {
                output::OutputFormat::Human
            };
            doctor::release_check_with_format(format)?
        }
        Command::Profiles { json } => {
            let format = if json {
                output::OutputFormat::Json
            } else {
                output::OutputFormat::Human
            };
            profile::list_profiles(format)?
        }
        Command::Versions => version::list_installed()?,
    }

    Ok(())
}
