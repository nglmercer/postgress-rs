pub mod eval;
pub mod query;
pub mod utils;

pub use eval::{evaluate_expr, evaluate_where};
pub use utils::build_select_messages;

use crate::protocol::parser::Parser;
use crate::protocol::backend::{BackendMessage, TransactionStatus};
use crate::protocol::FrontendMessage;
use crate::protocol::ExtendedQueryState;
use crate::buffer_cache::SharedBufferCache;
use crate::catalog::Catalog;
use crate::wal::WAL;
use crate::transaction::{TransactionManager, TransactionId};
use crate::storage::ephemeral::EphemeralStorage;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use query::handle_query;
use utils::{encode, send_error};

pub async fn run(port: u16, data_dir: String) -> anyhow::Result<()> {
    let storage: Arc<dyn crate::storage::StorageTrait> = if data_dir == ":memory:" || data_dir.is_empty() {
        Arc::new(EphemeralStorage::new())
    } else {
        Arc::new(crate::storage::mmap::MmapStorage::open(&data_dir, 8192).await?)
    };

    let wal = Arc::new(tokio::sync::Mutex::new(WAL::new(8192)));
    let cache = Arc::new(SharedBufferCache::new(storage.clone()));
    let catalog = Arc::new(Catalog::new(storage.clone()));
    let txn_mgr = Arc::new(TransactionManager::new());
    let lock_mgr = Arc::new(crate::transaction::locks::LockManager::new());

    catalog.register_cache(cache.clone());
    catalog.bootstrap().await?;
    cache.sync_from_catalog(&catalog);

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("postgress-rs listening on 127.0.0.1:{}", port);

    loop {
        let (socket, _) = listener.accept().await?;
        let cache = cache.clone();
        let catalog = catalog.clone();
        let wal = wal.clone();
        let txn_mgr = txn_mgr.clone();
        let lock_mgr = lock_mgr.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, cache, catalog, wal, txn_mgr, lock_mgr).await {
                tracing::error!("connection error: {}", e);
            }
        });
    }
}

pub async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    cache: Arc<SharedBufferCache>,
    catalog: Arc<Catalog>,
    wal: Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: Arc<TransactionManager>,
    lock_mgr: Arc<crate::transaction::locks::LockManager>,
) -> anyhow::Result<()> {
    let mut parser = Parser::new();
    let mut buf = vec![0u8; 8192];
    let mut current_xid: Option<TransactionId> = None;
    let mut ext_state = ExtendedQueryState::new();

    let startup_messages: Vec<BackendMessage> = vec![
        BackendMessage::BackendKeyData { pid: std::process::id(), secret: 12345 },
        BackendMessage::ParameterStatus { name: "client_encoding".to_string(), value: "UTF8".to_string() },
        BackendMessage::ParameterStatus { name: "server_version".to_string(), value: "170000".to_string() },
        BackendMessage::ParameterStatus { name: "server_encoding".to_string(), value: "UTF8".to_string() },
        BackendMessage::ParameterStatus { name: "DateStyle".to_string(), value: "ISO, MDY".to_string() },
        BackendMessage::ParameterStatus { name: "TimeZone".to_string(), value: "Etc/UTC".to_string() },
        BackendMessage::ParameterStatus { name: "integer_datetimes".to_string(), value: "on".to_string() },
        BackendMessage::ParameterStatus { name: "standard_conforming_strings".to_string(), value: "on".to_string() },
        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
    ];
    let _ = socket.write_all(&crate::protocol::encode(&startup_messages)).await;

    loop {
        let n = match socket.read(&mut buf).await {
            Ok(n) => {
                if n == 0 {
                    return Ok(());
                }
                n
            }
            Err(e) => return Err(e.into()),
        };

        if let Ok(messages) = FrontendMessage::decode(&buf[..n]) {
            for msg in messages {
                match msg {
                    FrontendMessage::Query { sql } => {
                        if let Some(query) = parser.feed(sql.as_bytes()) {
                            handle_query(&query, &catalog, &cache, &wal, &txn_mgr, &mut ext_state, &mut current_xid, &mut socket).await;
                        }
                    }
                    FrontendMessage::Parse { name, sql, parameter_types } => {
                        match ext_state.prepare(&name, &sql, parameter_types) {
                            Ok(()) => {
                                let _ = socket.write_all(&encode(BackendMessage::ParseComplete)).await;
                            }
                            Err(e) => {
                                send_error(&mut socket, e.to_string()).await;
                            }
                        }
                    }
                    FrontendMessage::Bind { portal, statement, parameter_formats: _, parameter_values, result_formats } => {
                        match ext_state.bind(&portal, &statement, parameter_values, result_formats) {
                            Ok(()) => {
                                let _ = socket.write_all(&encode(BackendMessage::BindComplete)).await;
                            }
                            Err(e) => {
                                send_error(&mut socket, e.to_string()).await;
                            }
                        }
                    }
                    FrontendMessage::Execute { portal, max_rows: _ } => {
                        if let Some(portal) = ext_state.get_portal(&portal) {
                            let query = portal.query.clone();
                            handle_query(&query, &catalog, &cache, &wal, &txn_mgr, &mut ext_state, &mut current_xid, &mut socket).await;
                        }
                    }
                    FrontendMessage::Sync => {
                        let tx_status = if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                        let _ = socket.write_all(&encode(BackendMessage::ReadyForQuery { status: tx_status })).await;
                    }
                    FrontendMessage::Describe { kind, name: _ } => {
                        if kind == b'S' {
                            let _ = socket.write_all(&encode(BackendMessage::NoData)).await;
                        } else {
                            let _ = socket.write_all(&encode(BackendMessage::NoData)).await;
                        }
                    }
                    FrontendMessage::Close { kind, name } => {
                        if kind == b'S' {
                            ext_state.close_statement(&name);
                        } else {
                            ext_state.close_portal(&name);
                        }
                        let _ = socket.write_all(&encode(BackendMessage::CloseComplete)).await;
                    }
                    FrontendMessage::Flush => {}
                    FrontendMessage::Terminate => {
                        ext_state.close_all();
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}
