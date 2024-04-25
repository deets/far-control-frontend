use clap::{ArgAction, Parser};

#[derive(Clone, Parser, Debug)]
#[clap(version, about, long_about = None)]
pub struct ProgramArgs {
    #[clap(short, long)]
    pub port: Option<String>,
    #[clap(short, long, action = ArgAction::SetTrue)]
    pub start_with_launch_control: bool,
    #[clap(short, long, action = ArgAction::SetTrue)]
    pub dont_record: bool,
}

impl Default for ProgramArgs {
    fn default() -> Self {
        Self {
            port: Default::default(),
            start_with_launch_control: false,
            dont_record: false,
        }
    }
}
