use std::fs;

use geomerge::ir::FlatTheory;
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("geomerge=info")),
        )
        .init();

    info!(path = "data/paths.json", "loading input theory");
    let input = fs::read_to_string("data/paths.json").unwrap();
    info!(bytes = input.len(), "read input file");
    let val: FlatTheory = serde_json::from_str(&input).unwrap();
    info!(
        table_count = val.tables.len(),
        law_count = val.laws.len(),
        "parsed theory"
    );
    println!("{:?}", val);
}
