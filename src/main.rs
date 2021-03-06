#![allow(clippy::redundant_closure)]

use log::info;
mod cli;

use crater::utils;
use failure::Fallible;
use std::panic;
use std::process;
use structopt::StructOpt;

fn main() {
    // Ignore errors loading `.env` file.
    let _ = dotenv::dotenv();

    // Initialize env_logger
    // This doesn't use from_default_env() because it doesn't allow to override filter_module()
    // with the RUST_LOG environment variable
    let mut env = env_logger::Builder::new();
    env.filter_module("crater", log::LevelFilter::Info);
    env.filter_module("rustwide", log::LevelFilter::Info);
    if let Ok(content) = std::env::var("RUST_LOG") {
        env.parse(&content);
    }
    rustwide::logging::init_with(env.build());

    let success = match panic::catch_unwind(main_) {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            utils::report_failure(&e);
            false
        }
        Err(e) => {
            utils::report_panic(&*e);
            false
        }
    };
    info!(
        "{}",
        if success {
            "command succeeded"
        } else {
            "command failed"
        }
    );
    process::exit(if success { 0 } else { 1 });
}

fn main_() -> Fallible<()> {
    cli::Crater::from_args().run()
}
