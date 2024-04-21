use clap::Parser;

use crate::observables::AdcGain;

#[derive(Clone, Parser, Debug)]
#[clap(version, about, long_about = None)]
pub struct ProgramArgs {
    #[clap(short, long)]
    pub port: Option<String>,
    #[clap(short, long, arg_enum)]
    pub gain: AdcGain,
}
