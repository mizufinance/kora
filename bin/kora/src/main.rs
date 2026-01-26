//! Kora - A minimal commonware + revm execution client.

mod cli;

fn main() -> eyre::Result<()> {
    use clap::Parser;

    kora_cli::Backtracing::enable();
    kora_cli::SigsegvHandler::install();

    let cli = cli::Cli::parse();

    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(cli.run())
}
