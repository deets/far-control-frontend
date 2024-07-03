use std::str::FromStr;

use clap::{ArgAction, Parser};

#[derive(Clone, Parser, Debug)]
pub enum LaunchMode {
    Observables,
    LaunchControl,
    RFSilence,
}

impl FromStr for LaunchMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "RFSilence" => Ok(LaunchMode::RFSilence),
            "LaunchControl" => Ok(LaunchMode::LaunchControl),
            "Observables" => Ok(LaunchMode::Observables),
            _ => Err("No valid value, use Observables, RFSilence, LaunchControl"),
        }
    }
}

#[derive(Clone, Parser, Debug)]
#[clap(version, about, long_about = None)]
pub struct ProgramArgs {
    #[clap(short, long)]
    pub port: Option<String>,
    #[clap(short, long)]
    pub start_with: LaunchMode,
    #[clap(short, long, action = ArgAction::SetTrue)]
    pub dont_record: bool,
}

impl Default for ProgramArgs {
    fn default() -> Self {
        Self {
            port: Default::default(),
            start_with: LaunchMode::Observables,
            dont_record: false,
        }
    }
}
