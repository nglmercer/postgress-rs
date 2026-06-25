use crate::buffer_cache::SharedBufferCache;
use crate::catalog::Catalog;
use crate::server;
use crate::storage::ephemeral::EphemeralStorage;
use crate::transaction::TransactionManager;
use crate::wal::WAL;
use clap::Parser;
use std::sync::Arc;
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
        let wal = Arc::new(tokio::sync::Mutex::new(WAL::new(8192)));
        let cache = Arc::new(SharedBufferCache::new(storage.clone()));
        let catalog = Arc::new(Catalog::new(storage.clone()));
        let txn_mgr = Arc::new(TransactionManager::new());
        let lock_mgr = Arc::new(crate::transaction::locks::LockManager::new());

        catalog.register_cache(cache.clone());
        catalog.bootstrap().await?;
        cache.sync_from_catalog(&catalog);

        let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port)).await?;
        tracing::info!("postgress-rs listening on 127.0.0.1:{}", args.port);

        loop {
            let (socket, _) = listener.accept().await?;
            let cache = cache.clone();
            let catalog = catalog.clone();
            let wal = wal.clone();
            let txn_mgr = txn_mgr.clone();
            let lock_mgr = lock_mgr.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    server::handle_connection(socket, cache, catalog, wal, txn_mgr, lock_mgr).await
                {
                    tracing::error!("connection error: {}", e);
                }
            });
        }
    }
}
