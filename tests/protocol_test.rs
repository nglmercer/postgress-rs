use postgress_rs::protocol::frontend::Message;
use postgress_rs::protocol::backend::{BackendMessage, TransactionStatus, FieldDescription, ErrorField, encode};
use postgress_rs::types::Oid;

fn make_frontend_msg(msg_type: u8, body: &[u8]) -> Vec<u8> {
    let mut data = vec![msg_type];
    let len = (body.len() + 4) as i32;
    data.extend_from_slice(&len.to_be_bytes());
    data.extend_from_slice(body);
    data
}

#[test]
fn test_decode_query_message() {
    let data = make_frontend_msg(b'Q', b"SELECT * FROM users;\0");
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Query { sql } => assert_eq!(sql, "SELECT * FROM users;"),
        _ => panic!("Expected Query message"),
    }
}

#[test]
fn test_decode_sync_message() {
    let data = make_frontend_msg(b'S', b"");
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], Message::Sync));
}

#[test]
fn test_decode_terminate_message() {
    let data = make_frontend_msg(b'X', b"");
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], Message::Terminate));
}

#[test]
fn test_decode_flush_message() {
    let data = make_frontend_msg(b'H', b"");
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], Message::Flush));
}

#[test]
fn test_decode_describe_statement() {
    let mut body = vec![b'S'];
    body.extend_from_slice(b"my_statement\0");
    let data = make_frontend_msg(b'D', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Describe { kind, name } => {
            assert_eq!(*kind, b'S');
            assert_eq!(name, "my_statement");
        }
        _ => panic!("Expected Describe message"),
    }
}

#[test]
fn test_decode_describe_portal() {
    let mut body = vec![b'P'];
    body.extend_from_slice(b"my_portal\0");
    let data = make_frontend_msg(b'D', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Describe { kind, name } => {
            assert_eq!(*kind, b'P');
            assert_eq!(name, "my_portal");
        }
        _ => panic!("Expected Describe message"),
    }
}

#[test]
fn test_decode_execute_message() {
    let mut body = Vec::new();
    body.extend_from_slice(b"portal1\0");
    body.extend_from_slice(&100_i32.to_be_bytes());
    let data = make_frontend_msg(b'E', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Execute { portal, max_rows } => {
            assert_eq!(portal, "portal1");
            assert_eq!(*max_rows, 100);
        }
        _ => panic!("Expected Execute message"),
    }
}

#[test]
fn test_decode_close_statement() {
    let mut body = vec![b'S'];
    body.extend_from_slice(b"stmt1\0");
    let data = make_frontend_msg(b'C', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Close { kind, name } => {
            assert_eq!(*kind, b'S');
            assert_eq!(name, "stmt1");
        }
        _ => panic!("Expected Close message"),
    }
}

#[test]
fn test_decode_parse_message() {
    let mut body = Vec::new();
    body.extend_from_slice(b"stmt1\0");
    body.extend_from_slice(b"SELECT 1\0");
    body.extend_from_slice(&2u16.to_be_bytes());
    body.extend_from_slice(&23u32.to_be_bytes());
    body.extend_from_slice(&25u32.to_be_bytes());
    let data = make_frontend_msg(b'P', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Parse { name, sql, parameter_types } => {
            assert_eq!(name, "stmt1");
            assert_eq!(sql, "SELECT 1");
            assert_eq!(parameter_types, &vec![23, 25]);
        }
        _ => panic!("Expected Parse message"),
    }
}

#[test]
fn test_decode_bind_message() {
    let mut body = Vec::new();
    body.extend_from_slice(b"p1\0");
    body.extend_from_slice(b"s1\0");
    body.extend_from_slice(&1u16.to_be_bytes());
    body.extend_from_slice(&0i16.to_be_bytes());
    body.extend_from_slice(&2u16.to_be_bytes());
    body.extend_from_slice(&5i32.to_be_bytes());
    body.extend_from_slice(b"hello");
    body.extend_from_slice(&5i32.to_be_bytes());
    body.extend_from_slice(b"world");
    body.extend_from_slice(&1u16.to_be_bytes());
    body.extend_from_slice(&0i16.to_be_bytes());
    let data = make_frontend_msg(b'B', &body);
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Bind { portal, statement, parameter_formats, parameter_values, result_formats } => {
            assert_eq!(portal, "p1");
            assert_eq!(statement, "s1");
            assert_eq!(parameter_formats, &vec![0]);
            assert_eq!(parameter_values.len(), 2);
            assert_eq!(parameter_values[0].as_deref(), Some(b"hello".as_ref()));
            assert_eq!(parameter_values[1].as_deref(), Some(b"world".as_ref()));
            assert_eq!(result_formats, &vec![0]);
        }
        _ => panic!("Expected Bind message"),
    }
}

#[test]
fn test_decode_bind_null_values() {
    let mut body = Vec::new();
    body.extend_from_slice(b"p1\0");
    body.extend_from_slice(b"s1\0");
    body.extend_from_slice(&0u16.to_be_bytes());
    body.extend_from_slice(&2u16.to_be_bytes());
    body.extend_from_slice(&(-1_i32).to_be_bytes());
    body.extend_from_slice(&5i32.to_be_bytes());
    body.extend_from_slice(b"hello");
    body.extend_from_slice(&0u16.to_be_bytes());
    let data = make_frontend_msg(b'B', &body);
    let messages = Message::decode(&data).unwrap();
    match &messages[0] {
        Message::Bind { parameter_values, .. } => {
            assert_eq!(parameter_values.len(), 2);
            assert_eq!(parameter_values[0], None);
            assert_eq!(parameter_values[1].as_deref(), Some(b"hello".as_ref()));
        }
        _ => panic!("Expected Bind message"),
    }
}

#[test]
fn test_decode_multiple_messages() {
    let mut data = Vec::new();
    data.extend_from_slice(&make_frontend_msg(b'Q', b"SELECT 1;\0"));
    data.extend_from_slice(&make_frontend_msg(b'S', b""));
    data.extend_from_slice(&make_frontend_msg(b'X', b""));
    let messages = Message::decode(&data).unwrap();
    assert_eq!(messages.len(), 3);
    assert!(matches!(messages[0], Message::Query { .. }));
    assert!(matches!(messages[1], Message::Sync));
    assert!(matches!(messages[2], Message::Terminate));
}

#[test]
fn test_decode_empty_input() {
    let messages = Message::decode(&[]).unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_decode_truncated_input() {
    let data = vec![b'Q', 0, 0];
    let messages = Message::decode(&data).unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_decode_unknown_message_type() {
    let data = make_frontend_msg(b'Z', b"");
    let messages = Message::decode(&data).unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_encode_transaction_status() {
    assert_eq!(TransactionStatus::Idle.as_u8(), b'I');
    assert_eq!(TransactionStatus::InTransaction.as_u8(), b'T');
    assert_eq!(TransactionStatus::InFailedTransaction.as_u8(), b'E');
}

#[test]
fn test_encode_auth_ok() {
    let encoded = BackendMessage::AuthenticationOk.encode();
    assert_eq!(encoded[0], b'R');
    assert_eq!(encoded.len(), 9);
    assert_eq!(i32::from_be_bytes([encoded[5], encoded[6], encoded[7], encoded[8]]), 0);
}

#[test]
fn test_encode_parameter_status() {
    let msg = BackendMessage::ParameterStatus {
        name: "client_encoding".to_string(),
        value: "UTF8".to_string(),
    };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'S');
    let body = String::from_utf8_lossy(&encoded[5..]);
    assert!(body.contains("client_encoding\0UTF8\0"));
}

#[test]
fn test_encode_backend_key_data() {
    let msg = BackendMessage::BackendKeyData { pid: 12345, secret: 67890 };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'K');
    assert_eq!(encoded.len(), 13);
    let pid = u32::from_be_bytes([encoded[5], encoded[6], encoded[7], encoded[8]]);
    let secret = u32::from_be_bytes([encoded[9], encoded[10], encoded[11], encoded[12]]);
    assert_eq!(pid, 12345);
    assert_eq!(secret, 67890);
}

#[test]
fn test_encode_ready_for_query() {
    let msg = BackendMessage::ReadyForQuery { status: TransactionStatus::Idle };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'Z');
    assert_eq!(encoded.len(), 6);
    assert_eq!(encoded[5], b'I');
}

#[test]
fn test_encode_row_description() {
    let msg = BackendMessage::RowDescription {
        fields: vec![FieldDescription {
            name: "id".to_string(),
            table_oid: Oid(100),
            column_attr: 1,
            type_oid: Oid(23),
            type_size: 4,
            type_mod: -1,
            format: 0,
        }],
    };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'T');
    let field_count = i16::from_be_bytes([encoded[5], encoded[6]]);
    assert_eq!(field_count, 1);
    assert_eq!(&encoded[7..9], b"id");
}

#[test]
fn test_encode_data_row() {
    let msg = BackendMessage::DataRow {
        values: vec![Some(b"hello".to_vec()), None, Some(b"world".to_vec())],
    };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'D');
    let value_count = i16::from_be_bytes([encoded[5], encoded[6]]);
    assert_eq!(value_count, 3);
}

#[test]
fn test_encode_data_row_empty() {
    let msg = BackendMessage::DataRow { values: vec![] };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'D');
    let value_count = i16::from_be_bytes([encoded[5], encoded[6]]);
    assert_eq!(value_count, 0);
}

#[test]
fn test_encode_command_complete() {
    let msg = BackendMessage::CommandComplete { tag: "INSERT 0 1".to_string() };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'C');
    let body = String::from_utf8_lossy(&encoded[5..encoded.len() - 1]);
    assert_eq!(body, "INSERT 0 1");
}

#[test]
fn test_encode_error_response() {
    let msg = BackendMessage::ErrorResponse {
        fields: vec![
            ErrorField { field_type: b'S', value: "ERROR".to_string() },
            ErrorField { field_type: b'M', value: "relation does not exist".to_string() },
        ],
    };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'E');
}

#[test]
fn test_encode_notice_response() {
    let msg = BackendMessage::NoticeResponse {
        fields: vec![
            ErrorField { field_type: b'S', value: "NOTICE".to_string() },
            ErrorField { field_type: b'M', value: "table created".to_string() },
        ],
    };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b'N');
}

#[test]
fn test_encode_parse_complete() {
    assert_eq!(BackendMessage::ParseComplete.encode(), vec![b'1', 0, 0, 0, 4]);
}

#[test]
fn test_encode_bind_complete() {
    assert_eq!(BackendMessage::BindComplete.encode(), vec![b'2', 0, 0, 0, 4]);
}

#[test]
fn test_encode_close_complete() {
    assert_eq!(BackendMessage::CloseComplete.encode(), vec![b'3', 0, 0, 0, 4]);
}

#[test]
fn test_encode_no_data() {
    assert_eq!(BackendMessage::NoData.encode(), vec![b'n', 0, 0, 0, 4]);
}

#[test]
fn test_encode_row_description_empty() {
    let encoded = BackendMessage::RowDescriptionEmpty.encode();
    assert_eq!(encoded[0], b'T');
    let field_count = i16::from_be_bytes([encoded[5], encoded[6]]);
    assert_eq!(field_count, 0);
}

#[test]
fn test_encode_parameter_description() {
    let msg = BackendMessage::ParameterDescription { types: vec![23, 25, 16] };
    let encoded = msg.encode();
    assert_eq!(encoded[0], b't');
    let param_count = i16::from_be_bytes([encoded[5], encoded[6]]);
    assert_eq!(param_count, 3);
}

#[test]
fn test_encode_multiple_messages() {
    let messages = vec![
        BackendMessage::AuthenticationOk,
        BackendMessage::ReadyForQuery { status: TransactionStatus::Idle },
    ];
    let encoded = encode(&messages);
    assert_eq!(encoded[0], b'R');
    assert_eq!(encoded[9], b'Z');
}
