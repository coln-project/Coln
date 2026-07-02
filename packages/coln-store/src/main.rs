// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_store::repl;
use tracing_subscriber::EnvFilter;
use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = false, hide = true)]
    enable_sql_mode: bool

}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("coln_store=info")),
        )
        .init();

    let args = Args::parse();
    repl::run(args.enable_sql_mode)
}
