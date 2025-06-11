use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(about = "Tool to convert cdk redb mint to sqlite", author = env!("CARGO_PKG_AUTHORS"), version = env!("CARGO_PKG_VERSION"))]
pub struct CLIArgs {
    #[arg(
        short,
        long,
        help = "Use the <directory> as the location of the database",
        required = false
    )]
    pub work_dir: Option<PathBuf>,
}
