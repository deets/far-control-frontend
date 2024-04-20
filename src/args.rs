use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
pub struct ProgramArgs {
    #[clap(short, long)]
    pub port: Option<String>,
}
