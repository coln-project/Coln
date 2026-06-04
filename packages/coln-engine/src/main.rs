use coln_engine::repl;
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("coln_engine=info")),
        )
        .init();

    repl::run()
}
