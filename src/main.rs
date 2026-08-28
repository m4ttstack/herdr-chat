use clap::{Parser, Subcommand};

mod deck;
mod herdr;
mod rt;
mod run;
mod state;
mod theme;
mod cmd {
    pub mod open_viewer;
    pub mod sign;
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
    /// Sign in to chat.
    SignIn,
    /// Sign out of chat.
    SignOut,
}

fn main() -> std::process::ExitCode {
    let runner = run::RealRunner;
    match Cli::parse().cmd {
        Cmd::OpenViewer { room } => cmd::open_viewer::run(&runner, room.as_deref()),
        Cmd::SignIn => match cmd::sign::run(&runner) {
            Ok(_) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("sign-in: {e}");
                std::process::ExitCode::FAILURE
            }
        },
        Cmd::SignOut => match cmd::sign::run_with(
            &runner,
            cmd::sign::Sign::Out,
            std::env::var("HERDR_PANE_ID").ok().as_deref(),
        ) {
            Ok(_) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("sign-out: {e}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
