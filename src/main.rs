use clap::Parser;
use postgress_rs::server::run;

#[derive(Parser, Debug)]
#[command(name = "postgress-rs", version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 5433)]
    port: u16,

    /// Data directory
    #[arg(short, long, default_value = "./pgdata")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    run(args.port, args.data_dir).await?;
    Ok(())
}
