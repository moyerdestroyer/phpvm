mod cli;
mod config;
mod doctor;
mod install;
mod manifest;
mod matrix;
mod net;
mod output;
mod profile;
mod profile_preset;
mod providers;
mod runner;
mod runtime_metadata;
mod shell_env;
#[cfg(test)]
mod testing;
mod version;

use anyhow::Result;
use clap::Parser;

use crate::cli::Command;
use crate::output::OutputFormat;

fn main() {
    if let Err(err) = run() {
        if let Some(exit) = err.downcast_ref::<runner::CommandExit>() {
            output::fatal_with_code(&err, exit.code);
        }
        output::fatal(&err);
    }
}

fn run() -> Result<()> {
    let args = cli::Args::parse();
    let format = args.command.output_format();

    match args.command {
        Command::Install { version, profile } => install::run(&version, profile.as_deref())?,
        Command::Run { version, command } => runner::run(&version, &command)?,
        Command::Matrix { command, .. } => matrix::run_with_format(&command, format)?,
        Command::Doctor { .. } => doctor::run_with_format(format)?,
        Command::ReleaseCheck { .. } => doctor::release_check_with_format(format)?,
        Command::Profile { command } => match command {
            cli::ProfileCommand::Use { name, version } => {
                profile::use_profile(&name, version.as_deref())?
            }
            cli::ProfileCommand::List { json } => {
                let list_format = if json {
                    OutputFormat::Json
                } else {
                    OutputFormat::Human
                };
                profile::list_profiles(list_format)?
            }
            cli::ProfileCommand::Edit { name, version } => {
                profile::edit_preset(name.as_deref(), version.as_deref())?
            }
        },
        Command::Ls => version::list_installed()?,
        Command::LsRemote => version::list_remote()?,
        Command::Info { version } => version::show_info(&version)?,
        Command::Use {
            version,
            profile,
            silent,
        } => version::run_use(version.as_deref(), profile.as_deref(), silent)?,
        Command::Deactivate { silent, persist } => version::deactivate(silent, persist)?,
        Command::Env { version, shell } => version::print_env(version.as_deref(), &shell)?,
    }

    Ok(())
}
