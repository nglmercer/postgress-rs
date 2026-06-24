use crate::protocol::parser::Parser;
use crate::protocol::codes::Query;
use crate::protocol::backend::{BackendMessage, TransactionStatus, ErrorField};
use crate::executor::{Plan, Planner};
use crate::executor::heap::{heap_scan, slow_scan, SlowScan, Filter, index_scan, tuple_update, tuple_delete, tuple_insert};
use crate::buffer_cache::SharedBufferCache;
use crate::catalog::Catalog;
use crate::wal::WAL;
use crate::storage::ephemeral::EphemeralStorage;
use crate::types::{Oid, Relation, ItemPointerData};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run(port: u16, data_dir: String) -> anyhow::Result<()> {
    let storage: Arc<dyn crate::storage::StorageTrait> = if data_dir == ":memory:" || data_dir.is_empty() {
        Arc::new(EphemeralStorage::new())
    } else {
        Arc::new(crate::storage::mmap::MmapStorage::open(&data_dir, 8192).await?)
    };

    let wal = Arc::new(parking_lot::Mutex::new(WAL::new(storage.clone(), 8192)));
    let cache = Arc::new(SharedBufferCache::new(storage.clone()));
    let catalog = Arc::new(Catalog::new(storage.clone()));

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
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, cache, catalog, wal).await {
                tracing::error!("connection error: {}", e);
            }
        });
    }
}

pub async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    cache: Arc<SharedBufferCache>,
    catalog: Arc<Catalog>,
    wal: Arc<parking_lot::Mutex<WAL>>,
) -> anyhow::Result<()> {
    let mut parser = Parser::new();
    let mut buf = vec![0u8; 8192];
    let mut in_transaction: bool = false;

    // Send standard backend startup messages on connection
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
                    }
                    let tx_status = if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("CREATE TABLE {}", name) },
                        BackendMessage::ReadyForQuery { status: tx_status },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::DropTable { name } => {
                    if let Err(e) = execute_drop_table(&catalog, name, &mut socket).await {
                        send_error(&mut socket, e.to_string()).await;
                    }
                    let tx_status = if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("DROP TABLE {}", name) },
                        BackendMessage::ReadyForQuery { status: tx_status },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Begin { mode: _ } => {
                    in_transaction = true;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "BEGIN".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::InTransaction },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Commit => {
                    wal.lock().append(&crate::wal::WALRecord::Commit { xid: 1 }).await?;
                    in_transaction = false;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Rollback => {
                    wal.lock().append(&crate::wal::WALRecord::Abort { xid: 1 }).await?;
                    in_transaction = false;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: "ROLLBACK".to_string() },
                        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Query::Insert { table, values } => {
                    let was_implicit_txn = !in_transaction;
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Begin { xid: 1 }).await?;
                        in_transaction = true;
                    }
                    let inserted = crate::executor::heap::tuple_insert(&cache, wal.as_ref(), *table, 0, values).await?;
                    let tx_status = if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("INSERT {}", inserted) },
                        BackendMessage::ReadyForQuery { status: tx_status },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Commit { xid: 1 }).await?;
                        in_transaction = false;
                        let commit_messages = vec![
                            BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                        ];
                        let _ = socket.write_all(&encode_messages(&commit_messages)).await;
                    }
                }
                Query::Update { table, column, value, where_clause } => {
                    let was_implicit_txn = !in_transaction;
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Begin { xid: 1 }).await?;
                        in_transaction = true;
                    }
                    let filter = where_clause.map(|s| parse_filter(&s)).transpose()?;
                    let updated = crate::executor::heap::tuple_update(&cache, wal.as_ref(), *table, 0, value, filter).await?;
                    let tx_status = if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("UPDATE {}", updated) },
                        BackendMessage::ReadyForQuery { status: tx_status },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Commit { xid: 1 }).await?;
                        in_transaction = false;
                        let commit_messages = vec![
                            BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                        ];
                        let _ = socket.write_all(&encode_messages(&commit_messages)).await;
                    }
                }
                Query::Delete { table, where_clause } => {
                    if !in_transaction {
                        wal.lock().append(&crate::wal::WALRecord::Begin { xid: 1 }).await?;
                        in_transaction = true;
                    }
                    let filter = where_clause.map(|s| parse_filter(&s)).transpose()?;
                    let deleted = crate::executor::heap::tuple_delete(&cache, wal.as_ref(), *table, filter).await?;
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("DELETE {}", deleted) },
                        BackendMessage::ReadyForQuery { status: if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                    if !in_transaction {
                        wal.lock().append(&crate::wal::WALRecord::Commit { xid: 1 }).await?;
                        in_transaction = false;
                        let commit_messages = vec![
                            BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                        ];
                        let _ = socket.write_all(&encode_messages(&commit_messages)).await;
                    }
                }
                Query::Select { table, where_clause, columns: _ } => {
                    let was_implicit_txn = !in_transaction;
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Begin { xid: 1 }).await?;
                        in_transaction = true;
                    }
                    let filter = where_clause.as_ref().map(|s| parse_filter(&s)).transpose()?;
                    let plan = Planner::plan(&query);
                    let tx_status = if in_transaction { TransactionStatus::InTransaction } else { TransactionStatus::Idle };
                    match plan {
                        crate::executor::Plan::SeqScan(scan) => {
                            if scan.filter.is_none() {
                                if let Err(e) = execute_seq_scan(&cache, scan.rel_oid, &mut socket, tx_status).await {
                                    send_error(&mut socket, e.to_string()).await;
                                }
                            } else {
                                if let Err(e) = execute_slow_scan(&cache, scan.rel_oid, scan.filter.clone().unwrap(), &mut socket, tx_status).await {
                                    send_error(&mut socket, e.to_string()).await;
                                }
                            }
                        }
                        crate::executor::Plan::IndexScan(_) => {
                            let messages: Vec<BackendMessage> = vec![
                                BackendMessage::CommandComplete { tag: "SELECT 0".to_string() },
                                BackendMessage::ReadyForQuery { status: tx_status },
                            ];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                    }
                    if was_implicit_txn {
                        wal.lock().append(&crate::wal::WALRecord::Commit { xid: 1 }).await?;
                        in_transaction = false;
                        let commit_messages = vec![
                            BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
                        ];
                        let _ = socket.write_all(&encode_messages(&commit_messages)).await;
                    }
                }
            }
        }
    }
}

async fn execute_create_table(
    catalog: &Catalog,
    cache: &SharedBufferCache,
    name: &str,
    columns: &[(String, Oid)],
    socket: &mut tokio::net::TcpStream,
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
    let rel_oid = catalog.create_relation(rel).await?;
    let messages = vec![
        BackendMessage::CommandComplete { tag: format!("CREATE TABLE {}", name) },
        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
    ];
    let _ = socket.write_all(&encode(messages)).await;
    Ok(())
}

async fn execute_drop_table(
    catalog: &Catalog,
    name: &str,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rels = catalog.list_relations();
    let found = rels.iter().find(|r| r.name == name).cloned();
    if let Some(rel) = found {
        catalog.delete_relation(rel.rel_oid).await?;
        let messages = vec![
            BackendMessage::CommandComplete { tag: format!("DROP TABLE {}", name) },
            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
        ];
        let _ = socket.write_all(&encode(messages)).await;
    } else {
        anyhow::bail!("relation \"{}\" does not exist", name);
    }
    Ok(())
}

async fn execute_seq_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    socket: &mut tokio::net::TcpStream,
    tx_status: TransactionStatus,
) -> anyhow::Result<()> {
    let rows = heap_scan(cache, rel_oid).await?;
    send_rows(cache, rel_oid, rows, socket, tx_status).await
}

async fn execute_slow_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    filter_str: String,
    socket: &mut tokio::net::TcpStream,
    tx_status: TransactionStatus,
) -> anyhow::Result<()> {
    let filter = parse_filter(&filter_str)?;
    let op = SlowScan { rel_oid: Oid(rel_oid), filter: Some(filter) };
    let rows = slow_scan(cache, &op).await?;
    send_rows(cache, rel_oid, rows, socket, tx_status).await
}

async fn send_rows(
    cache: &SharedBufferCache,
    rel_oid: u32,
    rows: Vec<(crate::types::ItemPointerData, Vec<String>)>,
    socket: &mut tokio::net::TcpStream,
    tx_status: TransactionStatus,
) -> anyhow::Result<()> {
    if !rows.is_empty() {
        let rel = cache.get_relation_mut(Oid(rel_oid))?.ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
        let mut messages: Vec<BackendMessage> = Vec::new();

        let field_descs: Vec<crate::protocol::backend::FieldDescription> = rel.relation.tuple_desc.fields.iter().map(|attr| {
            crate::protocol::backend::FieldDescription {
                name: attr.name.clone(),
                table_oid: rel.relation.rel_oid,
                column_attr: attr.attnum,
                type_oid: attr.type_oid,
                type_size: -1,
                type_mod: attr.typmod,
                format: 0,
            }
        }).collect();
        messages.push(BackendMessage::RowDescription { fields: field_descs });

        for (_tid, row) in rows.iter() {
            let values: Vec<Option<Vec<u8>>> = row.iter().map(|s| Some(s.as_bytes().to_vec())).collect();
            messages.push(BackendMessage::DataRow { values });
        }
        messages.push(BackendMessage::CommandComplete { tag: format!("SELECT {}", rows.len()) });
        messages.push(BackendMessage::ReadyForQuery { status: tx_status });
        let _ = socket.write_all(&encode_messages(&messages)).await;
    } else {
        let messages = vec![
            BackendMessage::RowDescriptionEmpty,
            BackendMessage::CommandComplete { tag: "SELECT 0".to_string() },
            BackendMessage::ReadyForQuery { status: tx_status },
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
    let column = parts[0].trim().parse::<usize>().or_else(|_| {
        Ok(0)
    })?;
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
