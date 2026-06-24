use crate::server;
use crate::buffer_cache::SharedBufferCache;
use crate::catalog::Catalog;
use crate::storage::ephemeral::EphemeralStorage;
use crate::storage::mmap::MmapStorage;
use crate::wal::WAL;
use crate::protocol::ResponseWriter;
use std::sync::Arc;
use std::path::Path;
use clap::Parser;
use tokio::net::TcpListener;

pub struct SuiteShell;

#[derive(Parser, Debug)]
#[command(name = "postgress-rs", version, about, long_about = None)]
struct ShellArgs {
    #[arg(short, long, default_value_t = 5433)]
    port: u16,

    #[arg(short, long, default_value = "./pgdata")]
    data_dir: String,
}

impl SuiteShell {
    pub async fn run() -> anyhow::Result<()> {
        let args = ShellArgs::parse();
        let storage: Arc<dyn crate::storage::StorageTrait> = Arc::new(EphemeralStorage::new());
        let wal = Arc::new(parking_lot::Mutex::new(WAL::new(storage.clone(), 8192)));
        let cache = Arc::new(SharedBufferCache::new(storage.clone()));
        let catalog = Arc::new(Catalog::new(storage.clone()));
        let writer = Arc::new(ResponseWriter::new());

        let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port)).await?;
        loop {
            let (socket, _) = listener.accept().await?;
            let cache = cache.clone();
            let catalog = catalog.clone();
            let wal = wal.clone();
            let writer = writer.clone();
            tokio::spawn(async move {
                server::handle_connection(socket, cache, catalog, wal, writer).await;
            });
        }
    }
}
