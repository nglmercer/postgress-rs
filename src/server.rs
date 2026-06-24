use crate::protocol::parser::Parser;
use crate::protocol::codes::Query;
use crate::protocol::backend::{BackendMessage, TransactionStatus, ErrorField};
use crate::protocol::FrontendMessage;
use crate::protocol::ExtendedQueryState;
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

async fn handle_query(
    query: &Query,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    _ext_state: &mut ExtendedQueryState,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    tracing::info!("executing: {:?}", query);
    match query {
        Query::CreateTable { name, columns } => {
            if let Err(e) = execute_create_table(catalog, cache, name, columns, socket).await {
                send_error(socket, e.to_string()).await;
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
            if let Err(e) = execute_drop_table(catalog, name, socket).await {
                send_error(socket, e.to_string()).await;
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
            if let Err(e) = execute_create_index(catalog, name, table, column, socket).await {
                send_error(socket, e.to_string()).await;
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
            *current_xid = Some(xid);
            let messages = vec![
                BackendMessage::CommandComplete { tag: "BEGIN".to_string() },
                BackendMessage::ReadyForQuery { status: TransactionStatus::InTransaction },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Query::Commit => {
            if let Some(xid) = current_xid {
                let _ = txn_mgr.commit(*xid);
                {
                    let wal_guard = wal.lock().await;
                    let _ = wal_guard.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
                }
                let _ = wal.lock().await.flush().await;
            }
            *current_xid = None;
            let messages = vec![
                BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
                BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Query::Rollback => {
            if let Some(xid) = current_xid {
                let _ = txn_mgr.rollback(*xid);
                {
                    let wal_guard = wal.lock().await;
                    let _ = wal_guard.append(&crate::wal::WALRecord::Abort { xid: xid.0 as u64 }).await;
                }
                let _ = wal.lock().await.flush().await;
            }
            *current_xid = None;
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
                *current_xid = Some(xid);
                let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
            }
            if let Err(e) = tuple_insert(cache, &*wal.lock().await, &TupleInsert { rel_oid: *table, values: values.clone() }).await {
                send_error(socket, e.to_string()).await;
            } else {
                let messages = vec![
                    BackendMessage::CommandComplete { tag: "INSERT 1".to_string() },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
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
                *current_xid = Some(xid);
                let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
            }
            let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose();
            match filter {
                Ok(filter) => {
                    match crate::executor::heap::tuple_update(cache, &*wal.lock().await, *table, 0, value, filter).await {
                        Ok(updated) => {
                            let messages = vec![
                                BackendMessage::CommandComplete { tag: format!("UPDATE {}", updated) },
                            ];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                        Err(e) => send_error(socket, e.to_string()).await,
                    }
                }
                Err(e) => send_error(socket, e.to_string()).await,
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
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
                *current_xid = Some(xid);
                let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
            }
            let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose();
            match filter {
                Ok(filter) => {
                    match crate::executor::heap::tuple_delete(cache, &*wal.lock().await, *table, filter).await {
                        Ok(deleted) => {
                            let messages = vec![
                                BackendMessage::CommandComplete { tag: format!("DELETE {}", deleted) },
                            ];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                        Err(e) => send_error(socket, e.to_string()).await,
                    }
                }
                Err(e) => send_error(socket, e.to_string()).await,
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![
                BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
            ];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Select { table: _, where_clause: _, columns: _ } => {
            let was_implicit_txn = current_xid.is_none();
            if was_implicit_txn {
                let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                *current_xid = Some(xid);
                let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
            }
            let plan = Planner::plan(query, &[]);
            match plan {
                crate::executor::Plan::SeqScan(scan) => {
                    if scan.filter.is_none() {
                        if let Err(e) = execute_seq_scan(cache, scan.rel_oid, socket).await {
                            send_error(socket, e.to_string()).await;
                        }
                    } else {
                        if let Err(e) = execute_slow_scan(cache, scan.rel_oid, scan.filter.clone().unwrap(), socket).await {
                            send_error(socket, e.to_string()).await;
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
                    let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![
                BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
            ];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Statement(stmt) => {
            handle_statement(stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
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

async fn handle_statement(
    stmt: &crate::sql::ast::Statement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    use crate::sql::ast::Statement;
    tracing::info!("executing statement: {:?}", stmt);
    match stmt {
        Statement::Select(select_stmt) => {
            handle_select_statement(select_stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
        Statement::Insert(insert_stmt) => {
            handle_insert_statement(insert_stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
        Statement::Update(update_stmt) => {
            handle_update_statement(update_stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
        Statement::Delete(delete_stmt) => {
            handle_delete_statement(delete_stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
        Statement::CreateTable(create_stmt) => {
            handle_create_table_statement(create_stmt, catalog, cache, socket).await;
        }
        Statement::CreateIndex(create_idx_stmt) => {
            handle_create_index_statement(create_idx_stmt, catalog, cache, socket).await;
        }
        Statement::CreateView(create_view_stmt) => {
            handle_create_view_statement(create_view_stmt, catalog, cache, socket).await;
        }
        Statement::AlterTable(alter_stmt) => {
            handle_alter_table_statement(alter_stmt, catalog, cache, socket).await;
        }
        Statement::DropTable(drop_stmt) => {
            handle_drop_table_statement(drop_stmt, catalog, cache, socket).await;
        }
        Statement::DropIndex(drop_idx_stmt) => {
            handle_drop_index_statement(drop_idx_stmt, catalog, cache, socket).await;
        }
        Statement::Begin(begin_stmt) => {
            handle_begin_statement(begin_stmt, txn_mgr, current_xid, socket).await;
        }
        Statement::Commit => {
            handle_commit_statement(txn_mgr, wal, current_xid, socket).await;
        }
        Statement::Rollback => {
            handle_rollback_statement(txn_mgr, wal, current_xid, socket).await;
        }
        Statement::Explain(inner_stmt) => {
            handle_explain_statement(inner_stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
        Statement::CreateSequence(_) => {
            send_error(socket, "CREATE SEQUENCE not yet supported".to_string()).await;
        }
        Statement::CreateType(_) => {
            send_error(socket, "CREATE TYPE not yet supported".to_string()).await;
        }
        Statement::Merge(_) => {
            send_error(socket, "MERGE not yet supported".to_string()).await;
        }
    }
}

async fn handle_select_statement(
    select: &crate::sql::ast::SelectStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    let was_implicit_txn = current_xid.is_none();
    if was_implicit_txn {
        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
        *current_xid = Some(xid);
        let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
    }

    // Handle WITH (CTE) clause - register temporary relations for each CTE
    let mut cte_oids = Vec::new();
    if let Some(ref with) = select.with {
        for cte in &with.ctes {
            // For now, just register the CTE name as a temporary relation
            // The actual materialization will happen when the CTE query is referenced
            let cte_oid = catalog.allocate_oid();
            let cte_rel = crate::types::Relation::empty(&cte.name, vec![]);
            let mut rel_with_oid = cte_rel;
            rel_with_oid.rel_oid = cte_oid;
            let _ = catalog.create_relation(rel_with_oid).await;
            cte_oids.push(cte_oid);
            tracing::info!("registered CTE '{}' with oid {}", cte.name, cte_oid.0);
        }
    }

    // Get table name from FROM clause
    if let Some(ref from) = select.from {
        if let Some(join) = from.joins.first() {
            match &join.table {
                crate::sql::ast::TableRef::Table(table_name) => {
                    let table_str = table_name.parts.join(".");
                    
                    // Try to find relation in catalog (including CTEs)
                    let rels = catalog.list_relations();
                    if let Some(rel) = rels.iter().find(|r| r.name.to_uppercase() == table_str.to_uppercase()) {
                        let rel_oid = rel.rel_oid.0;
                        
                        // Simple sequential scan for now
                        if let Err(e) = execute_seq_scan(cache, rel_oid, socket).await {
                            send_error(socket, e.to_string()).await;
                        }
                    } else {
                        send_error(socket, format!("relation \"{}\" does not exist", table_str)).await;
                    }
                }
                crate::sql::ast::TableRef::Subquery(_) => {
                    send_error(socket, "subqueries not yet supported".to_string()).await;
                }
                crate::sql::ast::TableRef::Function(_) => {
                    send_error(socket, "function calls not yet supported".to_string()).await;
                }
            }
        }
    } else {
        // SELECT without FROM - not supported yet
        send_error(socket, "SELECT without FROM not yet supported".to_string()).await;
    }

    // Clean up temporary CTE relations
    for cte_oid in cte_oids {
        let _ = catalog.delete_relation(cte_oid).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![
        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
    ];
    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
}

async fn handle_insert_statement(
    insert: &crate::sql::ast::InsertStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    let was_implicit_txn = current_xid.is_none();
    if was_implicit_txn {
        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
        *current_xid = Some(xid);
        let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
    }

    let table_str = insert.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels.iter().find(|r| r.name.to_uppercase() == table_str.to_uppercase()) {
        match &insert.source {
            crate::sql::ast::InsertSource::Values(rows) => {
                if let Some(row) = rows.first() {
                    let values: Vec<Vec<u8>> = row.iter().map(|expr| {
                        match expr {
                            crate::sql::ast::Expr::Literal(lit) => {
                                match lit {
                                    crate::sql::ast::Literal::String(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Number(n) => n.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Bool(b) => b.to_string().as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Null => vec![],
                                    crate::sql::ast::Literal::Blob(b) => b.clone(),
                                    crate::sql::ast::Literal::Date(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Time(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Timestamp(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::TimestampTz(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Interval(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Json(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::JsonB(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Uuid(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Money(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Bit(s) => s.as_bytes().to_vec(),
                                    crate::sql::ast::Literal::Hex(s) => s.as_bytes().to_vec(),
                                }
                            }
                            _ => format!("{:?}", expr).as_bytes().to_vec(),
                        }
                    }).collect();
                    
                    if let Err(e) = tuple_insert(cache, &*wal.lock().await, &TupleInsert { rel_oid: rel.rel_oid, values }).await {
                        send_error(socket, e.to_string()).await;
                    } else {
                        let messages = vec![
                            BackendMessage::CommandComplete { tag: "INSERT 1".to_string() },
                        ];
                        let _ = socket.write_all(&encode_messages(&messages)).await;
                    }
                }
            }
            crate::sql::ast::InsertSource::Select(_) => {
                send_error(socket, "INSERT...SELECT not yet supported".to_string()).await;
            }
        }
    } else {
        send_error(socket, format!("relation \"{}\" does not exist", table_str)).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![
        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
    ];
    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
}

async fn handle_update_statement(
    update: &crate::sql::ast::UpdateStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    let was_implicit_txn = current_xid.is_none();
    if was_implicit_txn {
        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
        *current_xid = Some(xid);
        let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
    }

    let table_str = update.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels.iter().find(|r| r.name.to_uppercase() == table_str.to_uppercase()) {
        if let Some(set_clause) = update.set_clauses.first() {
            let value = match &*set_clause.value {
                crate::sql::ast::Expr::Literal(lit) => {
                    match lit {
                        crate::sql::ast::Literal::String(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Number(n) => n.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Bool(b) => b.to_string().as_bytes().to_vec(),
                        crate::sql::ast::Literal::Null => vec![],
                        crate::sql::ast::Literal::Blob(b) => b.clone(),
                        crate::sql::ast::Literal::Date(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Time(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Timestamp(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::TimestampTz(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Interval(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Json(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::JsonB(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Uuid(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Money(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Bit(s) => s.as_bytes().to_vec(),
                        crate::sql::ast::Literal::Hex(s) => s.as_bytes().to_vec(),
                    }
                }
                _ => format!("{:?}", set_clause.value).as_bytes().to_vec(),
            };
            
            match crate::executor::heap::tuple_update(cache, &*wal.lock().await, rel.rel_oid, 0, &value, None).await {
                Ok(updated) => {
                    let messages = vec![
                        BackendMessage::CommandComplete { tag: format!("UPDATE {}", updated) },
                    ];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
                Err(e) => send_error(socket, e.to_string()).await,
            }
        }
    } else {
        send_error(socket, format!("relation \"{}\" does not exist", table_str)).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![
        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
    ];
    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
}

async fn handle_delete_statement(
    delete: &crate::sql::ast::DeleteStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    let was_implicit_txn = current_xid.is_none();
    if was_implicit_txn {
        let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
        *current_xid = Some(xid);
        let _ = wal.lock().await.append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 }).await;
    }

    let table_str = delete.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels.iter().find(|r| r.name.to_uppercase() == table_str.to_uppercase()) {
        match crate::executor::heap::tuple_delete(cache, &*wal.lock().await, rel.rel_oid, None).await {
            Ok(deleted) => {
                let messages = vec![
                    BackendMessage::CommandComplete { tag: format!("DELETE {}", deleted) },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
            Err(e) => send_error(socket, e.to_string()).await,
        }
    } else {
        send_error(socket, format!("relation \"{}\" does not exist", table_str)).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal.lock().await.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![
        BackendMessage::ReadyForQuery { status: if current_xid.is_some() { TransactionStatus::InTransaction } else { TransactionStatus::Idle } },
    ];
    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
}

async fn handle_create_table_statement(
    create: &crate::sql::ast::CreateTableStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    let table_name = create.table.parts.join(".");
    let columns: Vec<(String, Oid)> = create.columns.iter().map(|col| {
        let type_oid = match col.data_type {
            crate::sql::ast::DataType::Int => Oid(23),
            crate::sql::ast::DataType::BigInt => Oid(20),
            crate::sql::ast::DataType::SmallInt => Oid(21),
            crate::sql::ast::DataType::Text => Oid(25),
            crate::sql::ast::DataType::Varchar(_) => Oid(1043),
            crate::sql::ast::DataType::Boolean => Oid(16),
            _ => Oid(0),
        };
        (col.name.clone(), type_oid)
    }).collect();
    
    if let Err(e) = execute_create_table(catalog, cache, &table_name, &columns, socket).await {
        send_error(socket, e.to_string()).await;
    } else {
        let messages = vec![
            BackendMessage::CommandComplete { tag: format!("CREATE TABLE {}", table_name) },
            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
        ];
        let _ = socket.write_all(&encode_messages(&messages)).await;
    }
}

async fn handle_create_index_statement(
    create: &crate::sql::ast::CreateIndexStatement,
    catalog: &Catalog,
    _cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    let index_name = create.name.parts.join(".");
    let table_name = create.table.parts.join(".");
    let column_name = if let Some(col) = create.columns.first() {
        match &col.expr {
            crate::sql::ast::Expr::Identifier(id) => id.clone(),
            _ => "unknown".to_string(),
        }
    } else {
        "unknown".to_string()
    };
    
    if let Err(e) = execute_create_index(catalog, &index_name, &table_name, &column_name, socket).await {
        send_error(socket, e.to_string()).await;
    } else {
        let messages = vec![
            BackendMessage::CommandComplete { tag: format!("CREATE INDEX {}", index_name) },
            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
        ];
        let _ = socket.write_all(&encode_messages(&messages)).await;
    }
}

async fn handle_create_view_statement(
    _create: &crate::sql::ast::CreateViewStatement,
    _catalog: &Catalog,
    _cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    send_error(socket, "CREATE VIEW not yet supported".to_string()).await;
}

async fn handle_alter_table_statement(
    _alter: &crate::sql::ast::AlterTableStatement,
    _catalog: &Catalog,
    _cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    send_error(socket, "ALTER TABLE not yet supported".to_string()).await;
}

async fn handle_drop_table_statement(
    drop: &crate::sql::ast::DropTableStatement,
    catalog: &Catalog,
    _cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    let table_name = drop.table.parts.join(".");
    if let Err(e) = execute_drop_table(catalog, &table_name, socket).await {
        send_error(socket, e.to_string()).await;
    } else {
        let messages = vec![
            BackendMessage::CommandComplete { tag: format!("DROP TABLE {}", table_name) },
            BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
        ];
        let _ = socket.write_all(&encode_messages(&messages)).await;
    }
}

async fn handle_drop_index_statement(
    _drop: &crate::sql::ast::DropIndexStatement,
    _catalog: &Catalog,
    _cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    send_error(socket, "DROP INDEX not yet supported".to_string()).await;
}

async fn handle_begin_statement(
    begin: &crate::sql::ast::BeginStatement,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    let isolation_level = begin.isolation_level.as_ref().map(|il| {
        match il {
            crate::sql::ast::IsolationLevel::Serializable => IsolationLevel::Serializable,
            crate::sql::ast::IsolationLevel::RepeatableRead => IsolationLevel::RepeatableRead,
            crate::sql::ast::IsolationLevel::ReadCommitted => IsolationLevel::ReadCommitted,
            crate::sql::ast::IsolationLevel::ReadUncommitted => IsolationLevel::ReadUncommitted,
        }
    }).unwrap_or(IsolationLevel::ReadCommitted);
    
    let xid = txn_mgr.begin(isolation_level);
    *current_xid = Some(xid);
    let messages = vec![
        BackendMessage::CommandComplete { tag: "BEGIN".to_string() },
        BackendMessage::ReadyForQuery { status: TransactionStatus::InTransaction },
    ];
    let _ = socket.write_all(&encode_messages(&messages)).await;
}

async fn handle_commit_statement(
    txn_mgr: &Arc<TransactionManager>,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    if let Some(xid) = current_xid {
        let _ = txn_mgr.commit(*xid);
        {
            let wal_guard = wal.lock().await;
            let _ = wal_guard.append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 }).await;
        }
        let _ = wal.lock().await.flush().await;
    }
    *current_xid = None;
    let messages = vec![
        BackendMessage::CommandComplete { tag: "COMMIT".to_string() },
        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
    ];
    let _ = socket.write_all(&encode_messages(&messages)).await;
}

async fn handle_rollback_statement(
    txn_mgr: &Arc<TransactionManager>,
    wal: &Arc<tokio::sync::Mutex<WAL>>,
    current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    if let Some(xid) = current_xid {
        let _ = txn_mgr.rollback(*xid);
        {
            let wal_guard = wal.lock().await;
            let _ = wal_guard.append(&crate::wal::WALRecord::Abort { xid: xid.0 as u64 }).await;
        }
        let _ = wal.lock().await.flush().await;
    }
    *current_xid = None;
    let messages = vec![
        BackendMessage::CommandComplete { tag: "ROLLBACK".to_string() },
        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
    ];
    let _ = socket.write_all(&encode_messages(&messages)).await;
}

async fn handle_explain_statement(
    _inner: &crate::sql::ast::Statement,
    _catalog: &Catalog,
    _cache: &SharedBufferCache,
    _wal: &Arc<tokio::sync::Mutex<WAL>>,
    _txn_mgr: &Arc<TransactionManager>,
    _current_xid: &mut Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    send_error(socket, "EXPLAIN not yet supported".to_string()).await;
}

fn encode(msg: BackendMessage) -> Vec<u8> {
    crate::protocol::encode(&[msg])
}

fn encode_messages(messages: &[BackendMessage]) -> Vec<u8> {
    crate::protocol::encode(messages)
}
