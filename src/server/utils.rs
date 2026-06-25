use crate::buffer_cache::SharedBufferCache;
use crate::catalog::{Catalog, IndexInfo};
use crate::executor::heap::{heap_scan, slow_scan, Filter, SlowScan};
use crate::protocol::backend::{BackendMessage, ErrorField, TransactionStatus};
use crate::types::{ItemPointerData, Oid, PageId, Relation};
use tokio::io::AsyncWriteExt;

pub async fn execute_create_table(
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
        relpages: 0,
        reltuples: 0.0,
        relfrozenxid: 0,
    };
    catalog.create_relation(rel).await?;
    Ok(())
}

pub async fn execute_drop_table(
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

pub async fn execute_create_index(
    catalog: &Catalog,
    _name: &str,
    table: &str,
    column: &str,
    _socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rels = catalog.list_relations();
    let found = rels.iter().find(|r| r.name == table).cloned();
    let rel = found.ok_or_else(|| anyhow::anyhow!("relation \"{}\" does not exist", table))?;

    let root_page = PageId(catalog.allocate_oid().0);
    let index_info = IndexInfo {
        index_oid: catalog.allocate_oid(),
        rel_oid: rel.rel_oid,
        column_name: column.to_uppercase(),
        root_page,
    };

    catalog.register_index(index_info);
    Ok(())
}

pub async fn execute_seq_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let rows = heap_scan(cache, rel_oid).await?;
    send_rows(cache, rel_oid, rows, socket).await
}

pub async fn execute_slow_scan(
    cache: &SharedBufferCache,
    rel_oid: u32,
    filter_str: String,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let filter = parse_filter(&filter_str)?;
    let op = SlowScan {
        rel_oid: Oid(rel_oid),
        filter: Some(filter),
    };
    let rows = slow_scan(cache, &op).await?;
    send_rows(cache, rel_oid, rows, socket).await
}

pub fn build_select_messages(result: &crate::executor::SelectResult) -> Vec<BackendMessage> {
    let mut messages = Vec::new();

    if result.rows.is_empty() {
        messages.push(BackendMessage::RowDescriptionEmpty);
        messages.push(BackendMessage::CommandComplete {
            tag: "SELECT 0".to_string(),
        });
        return messages;
    }

    let field_descs: Vec<crate::protocol::backend::FieldDescription> = result
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| crate::protocol::backend::FieldDescription {
            name: name.clone(),
            table_oid: Oid(0),
            column_attr: (i as i16) + 1,
            type_oid: Oid(25),
            type_size: -1,
            type_mod: -1,
            format: 0,
        })
        .collect();

    messages.push(BackendMessage::RowDescription {
        fields: field_descs,
    });
    for row in &result.rows {
        let values: Vec<Option<Vec<u8>>> =
            row.iter().map(|s| Some(s.as_bytes().to_vec())).collect();
        messages.push(BackendMessage::DataRow { values });
    }
    messages.push(BackendMessage::CommandComplete {
        tag: format!("SELECT {}", result.rows.len()),
    });
    messages
}

pub async fn send_rows(
    cache: &SharedBufferCache,
    rel_oid: u32,
    rows: Vec<(ItemPointerData, Vec<String>)>,
    socket: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    if !rows.is_empty() {
        let (field_descs, data_rows) = {
            let state = cache
                .get_relation_state(Oid(rel_oid))
                .ok_or_else(|| anyhow::anyhow!("Relation not found"))?;
            let rel_state = state.lock();

            let field_descs: Vec<crate::protocol::backend::FieldDescription> = rel_state
                .relation
                .tuple_desc
                .fields
                .iter()
                .map(|attr| crate::protocol::backend::FieldDescription {
                    name: attr.name.clone(),
                    table_oid: rel_state.relation.rel_oid,
                    column_attr: attr.attnum,
                    type_oid: attr.type_oid,
                    type_size: -1,
                    type_mod: attr.typmod,
                    format: 0,
                })
                .collect();

            let data_rows: Vec<Vec<Option<Vec<u8>>>> = rows
                .iter()
                .map(|(_tid, row)| row.iter().map(|s| Some(s.as_bytes().to_vec())).collect())
                .collect();

            (field_descs, data_rows)
        };

        let mut messages: Vec<BackendMessage> = Vec::new();
        messages.push(BackendMessage::RowDescription {
            fields: field_descs,
        });
        for values in &data_rows {
            messages.push(BackendMessage::DataRow {
                values: values.clone(),
            });
        }
        messages.push(BackendMessage::CommandComplete {
            tag: format!("SELECT {}", rows.len()),
        });
        let _ = socket.write_all(&encode_messages(&messages)).await;
    } else {
        let messages = vec![
            BackendMessage::RowDescriptionEmpty,
            BackendMessage::CommandComplete {
                tag: "SELECT 0".to_string(),
            },
        ];
        let _ = socket.write_all(&encode_messages(&messages)).await;
    }
    Ok(())
}

pub fn parse_filter(s: &str) -> anyhow::Result<Filter> {
    let parts: Vec<&str> = s.splitn(3, '=').collect();
    if parts.len() != 3 {
        anyhow::bail!("unsupported filter format: {}", s);
    }
    let column = parts[0].trim().parse::<usize>().unwrap_or(0);
    let value = parts[2]
        .trim()
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .as_bytes()
        .to_vec();
    Ok(Filter { column, value })
}

pub async fn send_error(socket: &mut tokio::net::TcpStream, msg: String) {
    let err = BackendMessage::ErrorResponse {
        fields: vec![ErrorField {
            field_type: b'M',
            value: msg,
        }],
    };
    let _ = socket.write_all(&encode(err)).await;
    let ready = BackendMessage::ReadyForQuery {
        status: TransactionStatus::Idle,
    };
    let _ = socket.write_all(&encode(ready)).await;
}

pub fn encode(msg: BackendMessage) -> Vec<u8> {
    crate::protocol::encode(&[msg])
}

pub fn encode_messages(messages: &[BackendMessage]) -> Vec<u8> {
    crate::protocol::encode(messages)
}
