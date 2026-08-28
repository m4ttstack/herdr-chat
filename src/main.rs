use clap::{Parser, Subcommand};

mod deck;
mod herdr;
mod rt;
mod run;
mod state;
mod cmd {
    pub mod open_viewer;
}

#[derive(Parser)]
#[command(name = "herdr-chat")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open the chat web viewer (deck-sourced URL).
    OpenViewer {
        #[arg(long)]
        room: Option<String>,
    },
}

fn main() -> std::process::ExitCode {
    let runner = run::RealRunner;
    match Cli::parse().cmd {
        Cmd::OpenViewer { room } => cmd::open_viewer::run(&runner, room.as_deref()),
    }
}
