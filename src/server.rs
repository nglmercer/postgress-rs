use crate::protocol::parser::Parser;
use crate::protocol::codes::Query;
use crate::protocol::backend::{BackendMessage, TransactionStatus, ErrorField};
use crate::executor::Planner;
use crate::executor::heap::{heap_scan, slow_scan, SlowScan, Filter, tuple_insert, TupleInsert};
use crate::buffer_cache::SharedBufferCache;
use crate::catalog::{Catalog, IndexInfo};
use crate::wal::WAL;
use crate::transaction::{TransactionManager, IsolationLevel, TransactionId};
use crate::storage::ephemeral::EphemeralStorage;
use crate::types::{Oid, Relation};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, cache, catalog, wal, txn_mgr).await {
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
) -> anyhow::Result<()> {
    let mut parser = Parser::new();
    let mut buf = vec![0u8; 8192];
    let mut current_xid: Option<TransactionId> = None;

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

        if let Some(query) = parser.feed(&buf[..n]) {
            tracing::info!("executing: {:?}", query);
            match &query {
                Query::CreateTable { name, columns } => {
                    if let Err(e) = execute_create_table(&catalog, &cache, name, columns, &mut socket).await {
                        send_error(&mut socket, e.to_string()).await;
                    } else {
                        let tx_status = if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                        let messages = vec![
                            BackendMessage::CommandComplete { tag: format!("CREATE TABLE {}", name) },
                            BackendMessage::ReadyForQuery { status: tx_status },
                        ];
                        let _ = socket.write_all(&encode_messages(&messages)).await;
                    }
                }
                Query::DropTable { name } => {
                    if let Err(e) = execute_drop_table(&catalog, name, &mut socket).await {
                        send_error(&mut socket, e.to_string()).await;
                    } else {
                        let tx_status = if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                        let messages = vec![
                            BackendMessage::CommandComplete { tag: format!("DROP TABLE {}", name) },
                            BackendMessage::ReadyForQuery { status: tx_status },
                        ];
                        let _ = socket.write_all(&encode_messages(&messages)).await;
                    }
                }
                Query::CreateIndex { name, table, column } => {
                    if let Err(e) = execute_create_index(&catalog, name, table, column, &mut socket).await {
                        send_error(&mut socket, e.to_string()).await;
                    } else {
                        let tx_status = if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                        let messages = vec![
                            BackendMessage::CommandComplete { tag: format!("CREATE INDEX {}", name) },
                            BackendMessage::ReadyForQuery { status: tx_status },
                        ];
                        let _ = socket.write_all(&encode_messages(&messages)).await;
                    }
                }
                Query::Begin { mode: _ } => {
                    let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                    current_xid = Some(xid);
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "BEGIN".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::InTransaction },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Commit => {
                    if let Some(xid) = current_xid {
                        let _ = txn_mgr.commit(xid);
                        {
                            let wal_guard = wal.lock().await;
                            wal_guard.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await?;
                        }
                        wal.lock().await.flush().await?;
                    }
                    current_xid = None;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Rollback => {
                    if let Some(xid) = current_xid {
                        let _ = txn_mgr.rollback(xid);
                        {
                            let wal_guard = wal.lock().await;
                            wal_guard.append(&crate::wal::WALRecord::Abort { xid: xid.0 as u64 }).await?;
                        }
                        wal.lock().await.flush().await?;
                    }
                    current_xid = None;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "ROLLBACK".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Insert { table, values } => {
                    let was_implicit_txn = current_xid.is_none();
                    if was_implicit_txn {
                        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                        current_xid = Some(xid);
                        wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await?;
                    }
                    tuple_insert(&cache, &*wal.lock().await, &TupleInsert { rel_oid: *table, values: values.clone() }).await?;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "INSERT 1".to_string() },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if was_implicit_txn {
                        if let Some(xid) = current_xid {
                            wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await?;
                            let _ = txn_mgr.commit(xid);
                            wal.lock().await.flush().await?;
                        }
                        current_xid = None;
                    }
                    let ready_messages = vec![
                        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
                    ];
                    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
                }
                Query::Update { table, column: _, value, where_clause } => {
                    let was_implicit_txn = current_xid.is_none();
                    if was_implicit_txn {
                        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                        current_xid = Some(xid);
                        wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await?;
                    }
                    let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose()?;
                    let updated = crate::executor::heap::tuple_update(&cache, &*wal.lock().await, *table, 0, value, filter).await?;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("UPDATE {}", updated) },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if was_implicit_txn {
                        if let Some(xid) = current_xid {
                            wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await?;
                            let _ = txn_mgr.commit(xid);
                            wal.lock().await.flush().await?;
                        }
                        current_xid = None;
                    }
                    let ready_messages = vec![
                        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
                    ];
                    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
                }
                Query::Delete { table, where_clause } => {
                    let was_implicit_txn = current_xid.is_none();
                    if was_implicit_txn {
                        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                        current_xid = Some(xid);
                        wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await?;
                    }
                    let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose()?;
                    let deleted = crate::executor::heap::tuple_delete(&cache, &*wal.lock().await, *table, filter).await?;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("DELETE {}", deleted) },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if was_implicit_txn {
                        if let Some(xid) = current_xid {
                            wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await?;
                            let _ = txn_mgr.commit(xid);
                            wal.lock().await.flush().await?;
                        }
                        current_xid = None;
                    }
                    let ready_messages = vec![
                        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
                    ];
                    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
                }
                Query::Select { table: _, where_clause, columns: _ } => {
                    let was_implicit_txn = current_xid.is_none();
                    if was_implicit_txn {
                        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                        current_xid = Some(xid);
                        wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await?;
                    }
                    let _filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose()?;
                    let plan = Planner::plan(&query, &[]);
                    match plan {
                        crate::executor::Plan::SeqScan(scan) => {
                            if scan.filter.is_none() {
                                if let Err(e) = execute_seq_scan(&cache, scan.rel_oid, &mut socket).await {
                                    send_error(&mut socket, e.to_string()).await;
                                }
                            } else {
                                if let Err(e) = execute_slow_scan(&cache, scan.rel_oid, scan.filter.clone().unwrap(), &mut socket).await {
                                    send_error(&mut socket, e.to_string()).await;
                                }
                            }
                        }
                        crate::executor::Plan::IndexScan(_) => {
                            let messages: Vec<BackendMessage> = vec![
                                BackendMessage::CommandComplete { tag: "SELECT 0".to_string() },
                            ];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                    }
                    if was_implicit_txn {
                        if let Some(xid) = current_xid {
                            wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await?;
                            let _ = txn_mgr.commit(xid);
                            wal.lock().await.flush().await?;
                        }
                        current_xid = None;
                    }
                    let ready_messages = vec![
                        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
                    ];
                    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
                }
            }
        }
    }
}

async fn execute_create_table(
    catalog: &Catalog,
    _cache: &SharedBufferCache,
    name: &str,
    columns: &[(String, Oid)],
    _socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rel = Relation {
        rel_oid: Oid(0),
        name: name.to_string(),
        tuple_desc: crate::types::TupleDesc {
            fields: columns
                .iter()
                .enumerate()
                .map(|(i, (col_name, type_oid))| crate::types::Attribute {
                    name: col_name.clone(),
                    type_oid: *type_oid,
                    attnum: i as i16,
                    typmod: -1,
                })
                .collect(),
        },
        pages: vec![],
    };
    catalog.create_relation(rel).await?;
    Ok(())
}

async fn execute_drop_table(
    catalog: &Catalog,
    name: &str,
    _socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rels = catalog.list_relations();
    let found = rels.iter().find(|r| r.name == name).cloned();
    if let Some(rel) = found {
        catalog.delete_relation(rel.rel_oid).await?;
    } else {
        anyhow::bail!("relation \"{}\" does not exist", name);
    }
    Ok(())
}

async fn execute_create_index(
    catalog: &Catalog,
    _name: &str,
    table: &str,
    column: &str,
    _socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rels = catalog.list_relations();
    let found = rels.iter().find(|r| r.name == table).cloned();
    let rel = found.ok_or_else(|| anyhow::anyhow!("relation \"{}\" does not exist", table))?;

    let root_page = crate::types::PageId(catalog.allocate_oid().0);
    let index_info = IndexInfo {
        index_oid: catalog.allocate_oid(),
        rel_oid: rel.rel_oid,
        column_name: column.to_uppercase(),
        root_page,
    };

    catalog.register_index(index_info);
    Ok(())
}

async fn execute_seq_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rows = heap_scan(cache, rel_oid).await?;
    send_rows(cache, rel_oid, rows, socket).await
}

async fn execute_slow_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    filter_str: String,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let filter = parse_filter(&filter_str)?;
    let op = SlowScan { rel_oid: Oid(rel_oid), filter: Some(filter) };
    let rows = slow_scan(cache, &op).await?;
    send_rows(cache, rel_oid, rows, socket).await
}

async fn send_rows(
    cache: &SharedBufferCache,
    rel_oid: u32,
    rows: Vec<(crate::types::ItemPointerData, Vec<String>)>,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    if !rows.is_empty() {
        let (field_descs, data_rows) = {
            let state = cache.get_relation_state(Oid(rel_oid)).ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
            let rel_state = state.lock();

            let field_descs: Vec<crate::protocol::backend::FieldDescription> = rel_state.relation.tuple_desc.fields.iter().map(|attr| {
                crate::protocol::backend::FieldDescription {
                    name: attr.name.clone(),
                    table_oid: rel_state.relation.rel_oid,
                    column_attr: attr.attnum,
                    type_oid: attr.type_oid,
                    type_size: -1,
                    type_mod: attr.typmod,
                    format: 0,
                }
            }).collect();

            let data_rows: Vec<Vec<Option<Vec<u8>>>> = rows.iter().map(|(_tid, row)| {
                row.iter().map(|s| Some(s.as_bytes().to_vec())).collect()
            }).collect();

            (field_descs, data_rows)
        };

        let mut messages: Vec<BackendMessage> = Vec::new();
        messages.push(BackendMessage::RowDescription { fields: field_descs });
        for values in &data_rows {
            messages.push(BackendMessage::DataRow { values: values.clone() });
        }
        messages.push(BackendMessage::CommandComplete { tag: format!("SELECT {}", rows.len()) });
        let _ = socket.write_all(&encode_messages(&messages)).await;
    } else {
        let messages = vec![
            BackendMessage::RowDescriptionEmpty,
            BackendMessage::CommandComplete { tag: "SELECT 0".to_string() },
        ];
        let _ = socket.write_all(&encode_messages(&messages)).await;
    }
    Ok(())
}

fn parse_filter(s: &str) -> anyhow::Result<Filter> {
    let parts: Vec<&str> = s.splitn(3, '=').collect();
    if parts.len() != 3 {
        anyhow::bail!("unsupported filter format: {}", s);
    }
    let column = parts[0].trim().parse::<usize>().unwrap_or(0);
    let value = parts[2].trim().trim_start_matches('\'').trim_end_matches('\'').as_bytes().to_vec();
    Ok(Filter { column, value })
}

async fn send_error(socket: &mut tokio::net::TcpStream, msg: String) {
    let err = BackendMessage::ErrorResponse {
        fields: vec![ErrorField { field_type: b'M', value: msg }],
    };
    let _ = socket.write_all(&encode(err)).await;
    let ready = BackendMessage::ReadyForQuery { status: TransactionStatus::Idle };
    let _ = socket.write_all(&encode(ready)).await;
}

fn encode(msg: BackendMessage) -> Vec<u8> {
    crate::protocol::encode(&[msg])
}

fn encode_messages(messages: &[BackendMessage]) -> Vec<u8> {
    crate::protocol::encode(messages)
}
