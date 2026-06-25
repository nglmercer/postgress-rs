use crate::buffer_cache::SharedBufferCache;
use crate::catalog::Catalog;
use crate::executor::heap::{tuple_insert, TupleInsert};
use crate::executor::Planner;
use crate::protocol::backend::{BackendMessage, TransactionStatus};
use crate::protocol::codes::Query;
use crate::protocol::ExtendedQueryState;
use crate::transaction::{IsolationLevel, TransactionId, TransactionManager};
use crate::types::Oid;
use crate::wal::WAL;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

use crate::server::utils::{
    build_select_messages, encode_messages, execute_create_index, execute_create_table,
    execute_drop_table, execute_seq_scan, execute_slow_scan, parse_filter, send_error,
};

#[allow(clippy::too_many_arguments)]
pub async fn handle_query(
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
                let tx_status = if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                };
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: format!("CREATE TABLE {}", name),
                    },
                    BackendMessage::ReadyForQuery { status: tx_status },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
        }
        Query::DropTable { name } => {
            if let Err(e) = execute_drop_table(catalog, name, socket).await {
                send_error(socket, e.to_string()).await;
            } else {
                let tx_status = if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                };
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: format!("DROP TABLE {}", name),
                    },
                    BackendMessage::ReadyForQuery { status: tx_status },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
        }
        Query::CreateIndex {
            name,
            table,
            column,
        } => {
            if let Err(e) = execute_create_index(catalog, name, table, column, socket).await {
                send_error(socket, e.to_string()).await;
            } else {
                let tx_status = if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                };
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: format!("CREATE INDEX {}", name),
                    },
                    BackendMessage::ReadyForQuery { status: tx_status },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
        }
        Query::Begin { mode: _ } => {
            let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
            *current_xid = Some(xid);
            let messages = vec![
                BackendMessage::CommandComplete {
                    tag: "BEGIN".to_string(),
                },
                BackendMessage::ReadyForQuery {
                    status: TransactionStatus::InTransaction,
                },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Query::Commit => {
            if let Some(xid) = current_xid {
                let _ = txn_mgr.commit(*xid);
                {
                    let wal_guard = wal.lock().await;
                    let _ = wal_guard
                        .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                        .await;
                }
                let _ = wal.lock().await.flush().await;
            }
            *current_xid = None;
            let messages = vec![
                BackendMessage::CommandComplete {
                    tag: "COMMIT".to_string(),
                },
                BackendMessage::ReadyForQuery {
                    status: TransactionStatus::Idle,
                },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Query::Rollback => {
            if let Some(xid) = current_xid {
                let _ = txn_mgr.rollback(*xid);
                {
                    let wal_guard = wal.lock().await;
                    let _ = wal_guard
                        .append(&crate::wal::WALRecord::Abort { xid: xid.0 as u64 })
                        .await;
                }
                let _ = wal.lock().await.flush().await;
            }
            *current_xid = None;
            let messages = vec![
                BackendMessage::CommandComplete {
                    tag: "ROLLBACK".to_string(),
                },
                BackendMessage::ReadyForQuery {
                    status: TransactionStatus::Idle,
                },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Query::Insert { table, values } => {
            let was_implicit_txn = current_xid.is_none();
            if was_implicit_txn {
                let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                *current_xid = Some(xid);
                let _ = wal
                    .lock()
                    .await
                    .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
                    .await;
            }
            if let Err(e) = tuple_insert(
                cache,
                &*wal.lock().await,
                &TupleInsert {
                    rel_oid: *table,
                    values: values.clone(),
                },
            )
            .await
            {
                send_error(socket, e.to_string()).await;
            } else {
                let messages = vec![BackendMessage::CommandComplete {
                    tag: "INSERT 1".to_string(),
                }];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal
                        .lock()
                        .await
                        .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                        .await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![BackendMessage::ReadyForQuery {
                status: if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                },
            }];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Update {
            table,
            column: _,
            value,
            where_clause,
        } => {
            let was_implicit_txn = current_xid.is_none();
            if was_implicit_txn {
                let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                *current_xid = Some(xid);
                let _ = wal
                    .lock()
                    .await
                    .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
                    .await;
            }
            let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose();
            match filter {
                Ok(filter) => {
                    match crate::executor::heap::tuple_update(
                        cache,
                        &*wal.lock().await,
                        *table,
                        0,
                        value,
                        filter,
                    )
                    .await
                    {
                        Ok(updated) => {
                            let messages = vec![BackendMessage::CommandComplete {
                                tag: format!("UPDATE {}", updated),
                            }];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                        Err(e) => send_error(socket, e.to_string()).await,
                    }
                }
                Err(e) => send_error(socket, e.to_string()).await,
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal
                        .lock()
                        .await
                        .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                        .await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![BackendMessage::ReadyForQuery {
                status: if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                },
            }];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Delete {
            table,
            where_clause,
        } => {
            let was_implicit_txn = current_xid.is_none();
            if was_implicit_txn {
                let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                *current_xid = Some(xid);
                let _ = wal
                    .lock()
                    .await
                    .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
                    .await;
            }
            let filter = where_clause.as_ref().map(|s| parse_filter(s)).transpose();
            match filter {
                Ok(filter) => {
                    match crate::executor::heap::tuple_delete(
                        cache,
                        &*wal.lock().await,
                        *table,
                        filter,
                    )
                    .await
                    {
                        Ok(deleted) => {
                            let messages = vec![BackendMessage::CommandComplete {
                                tag: format!("DELETE {}", deleted),
                            }];
                            let _ = socket.write_all(&encode_messages(&messages)).await;
                        }
                        Err(e) => send_error(socket, e.to_string()).await,
                    }
                }
                Err(e) => send_error(socket, e.to_string()).await,
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal
                        .lock()
                        .await
                        .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                        .await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![BackendMessage::ReadyForQuery {
                status: if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                },
            }];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Select {
            table: _,
            where_clause: _,
            columns: _,
        } => {
            let was_implicit_txn = current_xid.is_none();
            if was_implicit_txn {
                let xid = txn_mgr.begin(IsolationLevel::ReadCommitted);
                *current_xid = Some(xid);
                let _ = wal
                    .lock()
                    .await
                    .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
                    .await;
            }
            let plan = Planner::plan(query, &[]);
            match plan {
                crate::executor::Plan::SeqScan(scan) => {
                    if scan.filter.is_none() {
                        if let Err(e) = execute_seq_scan(cache, scan.rel_oid, socket).await {
                            send_error(socket, e.to_string()).await;
                        }
                    } else {
                        if let Err(e) = execute_slow_scan(
                            cache,
                            scan.rel_oid,
                            scan.filter.clone().unwrap(),
                            socket,
                        )
                        .await
                        {
                            send_error(socket, e.to_string()).await;
                        }
                    }
                }
                crate::executor::Plan::IndexScan(_) => {
                    let messages: Vec<BackendMessage> = vec![BackendMessage::CommandComplete {
                        tag: "SELECT 0".to_string(),
                    }];
                    let _ = socket.write_all(&encode_messages(&messages)).await;
                }
            }
            if was_implicit_txn {
                if let Some(xid) = current_xid {
                    let _ = wal
                        .lock()
                        .await
                        .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                        .await;
                    let _ = txn_mgr.commit(*xid);
                    let _ = wal.lock().await.flush().await;
                }
                *current_xid = None;
            }
            let ready_messages = vec![BackendMessage::ReadyForQuery {
                status: if current_xid.is_some() {
                    TransactionStatus::InTransaction
                } else {
                    TransactionStatus::Idle
                },
            }];
            let _ = socket.write_all(&encode_messages(&ready_messages)).await;
        }
        Query::Statement(stmt) => {
            handle_statement(stmt, catalog, cache, wal, txn_mgr, current_xid, socket).await;
        }
    }
}

pub async fn handle_statement(
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
            handle_select_statement(
                select_stmt,
                catalog,
                cache,
                wal,
                txn_mgr,
                current_xid,
                socket,
            )
            .await;
        }
        Statement::Insert(insert_stmt) => {
            handle_insert_statement(
                insert_stmt,
                catalog,
                cache,
                wal,
                txn_mgr,
                current_xid,
                socket,
            )
            .await;
        }
        Statement::Update(update_stmt) => {
            handle_update_statement(
                update_stmt,
                catalog,
                cache,
                wal,
                txn_mgr,
                current_xid,
                socket,
            )
            .await;
        }
        Statement::Delete(delete_stmt) => {
            handle_delete_statement(
                delete_stmt,
                catalog,
                cache,
                wal,
                txn_mgr,
                current_xid,
                socket,
            )
            .await;
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
            handle_explain_statement(
                inner_stmt,
                catalog,
                cache,
                wal,
                txn_mgr,
                current_xid,
                socket,
            )
            .await;
        }
        Statement::CreateSequence(_) => {
            send_error(socket, "CREATE SEQUENCE not yet supported".to_string()).await;
        }
        Statement::CreateType(_) => {
            send_error(socket, "CREATE TYPE not yet supported".to_string()).await;
        }
        Statement::CreateMaterializedView(_) => {
            send_error(
                socket,
                "CREATE MATERIALIZED VIEW not yet supported".to_string(),
            )
            .await;
        }
        Statement::CreateSchema(_) => {
            send_error(socket, "CREATE SCHEMA not yet supported".to_string()).await;
        }
        Statement::Set(set_stmt) => {
            handle_set_statement(set_stmt, txn_mgr, current_xid, socket).await;
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
        let _ = wal
            .lock()
            .await
            .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
            .await;
    }

    let snapshot = current_xid.map(|xid| txn_mgr.get_snapshot_for_statement(xid));

    // Handle WITH (CTE) clause - register temporary relations for each CTE
    let mut cte_oids = Vec::new();
    if let Some(ref with) = select.with {
        for cte in &with.ctes {
            let cte_oid = catalog.allocate_oid();
            let cte_rel = crate::types::Relation::empty(&cte.name, vec![]);
            let mut rel_with_oid = cte_rel;
            rel_with_oid.rel_oid = cte_oid;
            let _ = catalog.create_relation(rel_with_oid).await;
            cte_oids.push(cte_oid);
            tracing::info!("registered CTE '{}' with oid {}", cte.name, cte_oid.0);
        }
    }

    match crate::executor::select::execute_select_with_snapshot(select, cache, catalog, snapshot)
        .await
    {
        Ok(result) => {
            let messages = build_select_messages(&result);
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
        Err(e) => {
            send_error(socket, e.to_string()).await;
        }
    }

    // Clean up temporary CTE relations
    for cte_oid in cte_oids {
        let _ = catalog.delete_relation(cte_oid).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal
                .lock()
                .await
                .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                .await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![BackendMessage::ReadyForQuery {
        status: if current_xid.is_some() {
            TransactionStatus::InTransaction
        } else {
            TransactionStatus::Idle
        },
    }];
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
        let _ = wal
            .lock()
            .await
            .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
            .await;
    }

    let table_str = insert.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels
        .iter()
        .find(|r| r.name.to_uppercase() == table_str.to_uppercase())
    {
        match &insert.source {
            crate::sql::ast::InsertSource::Values(rows) => {
                if let Some(row) = rows.first() {
                    let values: Vec<Vec<u8>> = row
                        .iter()
                        .map(|expr| match expr {
                            crate::sql::ast::Expr::Literal(lit) => match lit {
                                crate::sql::ast::Literal::String(s) => s.as_bytes().to_vec(),
                                crate::sql::ast::Literal::Number(n) => n.as_bytes().to_vec(),
                                crate::sql::ast::Literal::Bool(b) => {
                                    b.to_string().as_bytes().to_vec()
                                }
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
                            },
                            _ => format!("{:?}", expr).as_bytes().to_vec(),
                        })
                        .collect();

                    if let Err(e) = tuple_insert(
                        cache,
                        &*wal.lock().await,
                        &TupleInsert {
                            rel_oid: rel.rel_oid,
                            values,
                        },
                    )
                    .await
                    {
                        send_error(socket, e.to_string()).await;
                    } else {
                        let messages = vec![BackendMessage::CommandComplete {
                            tag: "INSERT 1".to_string(),
                        }];
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
            let _ = wal
                .lock()
                .await
                .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                .await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![BackendMessage::ReadyForQuery {
        status: if current_xid.is_some() {
            TransactionStatus::InTransaction
        } else {
            TransactionStatus::Idle
        },
    }];
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
        let _ = wal
            .lock()
            .await
            .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
            .await;
    }

    let table_str = update.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels
        .iter()
        .find(|r| r.name.to_uppercase() == table_str.to_uppercase())
    {
        if let Some(set_clause) = update.set_clauses.first() {
            let value = match &*set_clause.value {
                crate::sql::ast::Expr::Literal(lit) => match lit {
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
                },
                _ => format!("{:?}", set_clause.value).as_bytes().to_vec(),
            };

            match crate::executor::heap::tuple_update(
                cache,
                &*wal.lock().await,
                rel.rel_oid,
                0,
                &value,
                None,
            )
            .await
            {
                Ok(updated) => {
                    let messages = vec![BackendMessage::CommandComplete {
                        tag: format!("UPDATE {}", updated),
                    }];
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
            let _ = wal
                .lock()
                .await
                .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                .await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![BackendMessage::ReadyForQuery {
        status: if current_xid.is_some() {
            TransactionStatus::InTransaction
        } else {
            TransactionStatus::Idle
        },
    }];
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
        let _ = wal
            .lock()
            .await
            .append(&crate::wal::WALRecord::Begin { xid: xid.0 as u64 })
            .await;
    }

    let table_str = delete.table.parts.join(".");
    let rels = catalog.list_relations();
    if let Some(rel) = rels
        .iter()
        .find(|r| r.name.to_uppercase() == table_str.to_uppercase())
    {
        match crate::executor::heap::tuple_delete(cache, &*wal.lock().await, rel.rel_oid, None)
            .await
        {
            Ok(deleted) => {
                let messages = vec![BackendMessage::CommandComplete {
                    tag: format!("DELETE {}", deleted),
                }];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
            Err(e) => send_error(socket, e.to_string()).await,
        }
    } else {
        send_error(socket, format!("relation \"{}\" does not exist", table_str)).await;
    }

    if was_implicit_txn {
        if let Some(xid) = current_xid {
            let _ = wal
                .lock()
                .await
                .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                .await;
            let _ = txn_mgr.commit(*xid);
            let _ = wal.lock().await.flush().await;
        }
        *current_xid = None;
    }
    let ready_messages = vec![BackendMessage::ReadyForQuery {
        status: if current_xid.is_some() {
            TransactionStatus::InTransaction
        } else {
            TransactionStatus::Idle
        },
    }];
    let _ = socket.write_all(&encode_messages(&ready_messages)).await;
}

async fn handle_create_table_statement(
    create: &crate::sql::ast::CreateTableStatement,
    catalog: &Catalog,
    cache: &SharedBufferCache,
    socket: &mut tokio::net::TcpStream,
) {
    let table_name = create.table.parts.join(".");
    let columns: Vec<(String, Oid)> = create
        .columns
        .iter()
        .map(|col| {
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
        })
        .collect();

    if let Err(e) = execute_create_table(catalog, cache, &table_name, &columns, socket).await {
        send_error(socket, e.to_string()).await;
    } else {
        let messages = vec![
            BackendMessage::CommandComplete {
                tag: format!("CREATE TABLE {}", table_name),
            },
            BackendMessage::ReadyForQuery {
                status: TransactionStatus::Idle,
            },
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

    if let Err(e) =
        execute_create_index(catalog, &index_name, &table_name, &column_name, socket).await
    {
        send_error(socket, e.to_string()).await;
    } else {
        let messages = vec![
            BackendMessage::CommandComplete {
                tag: format!("CREATE INDEX {}", index_name),
            },
            BackendMessage::ReadyForQuery {
                status: TransactionStatus::Idle,
            },
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
            BackendMessage::CommandComplete {
                tag: format!("DROP TABLE {}", table_name),
            },
            BackendMessage::ReadyForQuery {
                status: TransactionStatus::Idle,
            },
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
    let isolation_level = begin
        .isolation_level
        .as_ref()
        .map(|il| match il {
            crate::sql::ast::IsolationLevel::Serializable => IsolationLevel::Serializable,
            crate::sql::ast::IsolationLevel::RepeatableRead => IsolationLevel::RepeatableRead,
            crate::sql::ast::IsolationLevel::ReadCommitted => IsolationLevel::ReadCommitted,
            crate::sql::ast::IsolationLevel::ReadUncommitted => IsolationLevel::ReadUncommitted,
        })
        .unwrap_or(IsolationLevel::ReadCommitted);

    let xid = txn_mgr.begin(isolation_level);
    *current_xid = Some(xid);
    let messages = vec![
        BackendMessage::CommandComplete {
            tag: "BEGIN".to_string(),
        },
        BackendMessage::ReadyForQuery {
            status: TransactionStatus::InTransaction,
        },
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
            let _ = wal_guard
                .append(&crate::wal::WALRecord::Commit { xid: xid.0 as u64 })
                .await;
        }
        let _ = wal.lock().await.flush().await;
    }
    *current_xid = None;
    let messages = vec![
        BackendMessage::CommandComplete {
            tag: "COMMIT".to_string(),
        },
        BackendMessage::ReadyForQuery {
            status: TransactionStatus::Idle,
        },
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
            let _ = wal_guard
                .append(&crate::wal::WALRecord::Abort { xid: xid.0 as u64 })
                .await;
        }
        let _ = wal.lock().await.flush().await;
    }
    *current_xid = None;
    let messages = vec![
        BackendMessage::CommandComplete {
            tag: "ROLLBACK".to_string(),
        },
        BackendMessage::ReadyForQuery {
            status: TransactionStatus::Idle,
        },
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

async fn handle_set_statement(
    set_stmt: &crate::sql::ast::SetStatement,
    txn_mgr: &Arc<TransactionManager>,
    current_xid: &Option<TransactionId>,
    socket: &mut tokio::net::TcpStream,
) {
    use crate::sql::ast::SetValue;

    let name = set_stmt.name.to_lowercase();
    match name.as_str() {
        "statement_timeout" => {
            if let Some(xid) = current_xid {
                let timeout_ms = match set_stmt.values.first() {
                    Some(SetValue::Number(n)) => n.parse::<u64>().unwrap_or(0),
                    Some(SetValue::Default) => 0,
                    _ => 0,
                };
                let timeout = if timeout_ms == 0 {
                    None
                } else {
                    Some(std::time::Duration::from_millis(timeout_ms))
                };
                if let Some(mut txn) = txn_mgr.get_transaction(*xid) {
                    txn.timeout_config.statement_timeout = timeout;
                }
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: "SET".to_string(),
                    },
                    BackendMessage::ReadyForQuery {
                        status: TransactionStatus::InTransaction,
                    },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            } else {
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: "SET".to_string(),
                    },
                    BackendMessage::ReadyForQuery {
                        status: TransactionStatus::Idle,
                    },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
        }
        "lock_timeout" => {
            if let Some(xid) = current_xid {
                let timeout_ms = match set_stmt.values.first() {
                    Some(SetValue::Number(n)) => n.parse::<u64>().unwrap_or(0),
                    Some(SetValue::Default) => 0,
                    _ => 0,
                };
                let timeout = if timeout_ms == 0 {
                    None
                } else {
                    Some(std::time::Duration::from_millis(timeout_ms))
                };
                if let Some(mut txn) = txn_mgr.get_transaction(*xid) {
                    txn.timeout_config.lock_timeout = timeout;
                }
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: "SET".to_string(),
                    },
                    BackendMessage::ReadyForQuery {
                        status: TransactionStatus::InTransaction,
                    },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            } else {
                let messages = vec![
                    BackendMessage::CommandComplete {
                        tag: "SET".to_string(),
                    },
                    BackendMessage::ReadyForQuery {
                        status: TransactionStatus::Idle,
                    },
                ];
                let _ = socket.write_all(&encode_messages(&messages)).await;
            }
        }
        _ => {
            let messages = vec![
                BackendMessage::CommandComplete {
                    tag: "SET".to_string(),
                },
                BackendMessage::ReadyForQuery {
                    status: if current_xid.is_some() {
                        TransactionStatus::InTransaction
                    } else {
                        TransactionStatus::Idle
                    },
                },
            ];
            let _ = socket.write_all(&encode_messages(&messages)).await;
        }
    }
}
