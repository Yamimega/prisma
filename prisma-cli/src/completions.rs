use clap::CommandFactory;
use clap_complete::Shell;

use crate::Cli;

pub fn generate(shell: Shell) {
    clap_complete::generate(shell, &mut Cli::command(), "prisma", &mut std::io::stdout());
}
