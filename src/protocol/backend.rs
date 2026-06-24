use crate::types::Oid;

#[derive(Debug, Clone, Copy)]
pub enum TransactionStatus {
    Idle,
    InTransaction,
    InFailedTransaction,
}

impl TransactionStatus {
    pub fn as_u8(&self) -> u8 {
        match self {
            TransactionStatus::Idle => b'I',
            TransactionStatus::InTransaction => b'T',
            TransactionStatus::InFailedTransaction => b'E',
        }
    }
}

#[derive(Debug)]
pub enum BackendMessage {
    AuthenticationOk,
    ParameterStatus { name: String, value: String },
    BackendKeyData { pid: u32, secret: u32 },
    ReadyForQuery { status: TransactionStatus },
    RowDescription { fields: Vec<FieldDescription> },
    DataRow { values: Vec<Option<Vec<u8>>> },
    CommandComplete { tag: String },
    ErrorResponse { fields: Vec<ErrorField> },
    NoticeResponse { fields: Vec<ErrorField> },
    ParseComplete,
    BindComplete,
    CloseComplete,
    NoData,
    ParameterDescription { types: Vec<u32> },
    RowDescriptionEmpty,
}

#[derive(Debug, Clone)]
pub struct FieldDescription {
    pub name: String,
    pub table_oid: Oid,
    pub column_attr: i16,
    pub type_oid: Oid,
    pub type_size: i16,
    pub type_mod: i32,
    pub format: i16,
}

#[derive(Debug, Clone)]
pub struct ErrorField {
    pub field_type: u8,
    pub value: String,
}

impl BackendMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            BackendMessage::AuthenticationOk => {
                let mut msg = vec![b'R'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg.extend_from_slice(&(0_u32.to_be_bytes()));
                msg
            }
            BackendMessage::ParameterStatus { name, value } => {
                let mut msg = vec![b'S'];
                let body = format!("{}\0{}\0", name, value);
                msg.extend_from_slice(&((body.len() as i32 + 4).to_be_bytes()));
                msg.extend_from_slice(body.as_bytes());
                msg
            }
            BackendMessage::BackendKeyData { pid, secret } => {
                let mut msg = vec![b'K'];
                msg.extend_from_slice(&(12_i32.to_be_bytes()));
                msg.extend_from_slice(&pid.to_be_bytes());
                msg.extend_from_slice(&secret.to_be_bytes());
                msg
            }
            BackendMessage::ReadyForQuery { status } => {
                let mut msg = vec![b'Z'];
                msg.extend_from_slice(&(5_i32.to_be_bytes()));
                msg.push(status.as_u8());
                msg
            }
            BackendMessage::RowDescription { fields } => {
                let mut msg = vec![b'T'];
                let mut body = Vec::new();
                body.extend_from_slice(&(fields.len() as i16).to_be_bytes());
                for f in fields {
                    body.extend_from_slice(f.name.as_bytes());
                    body.push(0);
                    body.extend_from_slice(&f.table_oid.0.to_be_bytes());
                    body.extend_from_slice(&f.column_attr.to_be_bytes());
                    body.extend_from_slice(&f.type_oid.0.to_be_bytes());
                    body.extend_from_slice(&f.type_size.to_be_bytes());
                    body.extend_from_slice(&f.type_mod.to_be_bytes());
                    body.extend_from_slice(&(0_i16).to_be_bytes());
                }
                msg.extend_from_slice(&((body.len() + 4) as i32).to_be_bytes());
                msg.extend_from_slice(&body);
                msg
            }
            BackendMessage::DataRow { values } => {
                let mut msg = vec![b'D'];
                let mut body = Vec::new();
                body.extend_from_slice(&(values.len() as i16).to_be_bytes());
                for v in values {
                    match v {
                        Some(data) => {
                            body.extend_from_slice(&(data.len() as i32).to_be_bytes());
                            body.extend_from_slice(data);
                        }
                        None => {
                            body.extend_from_slice(&((-1_i32).to_be_bytes()));
                        }
                    }
                }
                msg.extend_from_slice(&((body.len() + 4) as i32).to_be_bytes());
                msg.extend_from_slice(&body);
                msg
            }
            BackendMessage::CommandComplete { tag } => {
                let mut msg = vec![b'C'];
                let body = format!("{}\0", tag);
                msg.extend_from_slice(&((body.len() as i32 + 4).to_be_bytes()));
                msg.extend_from_slice(body.as_bytes());
                msg
            }
            BackendMessage::ErrorResponse { fields } => {
                let mut msg = vec![b'E'];
                let mut body = Vec::new();
                for f in fields {
                    body.push(f.field_type);
                    body.extend_from_slice(f.value.as_bytes());
                    body.push(0);
                }
                body.push(0);
                msg.extend_from_slice(&((body.len() + 4) as i32).to_be_bytes());
                msg.extend_from_slice(&body);
                msg
            }
            BackendMessage::NoticeResponse { fields } => {
                let mut msg = vec![b'N'];
                let mut body = Vec::new();
                for f in fields {
                    body.push(f.field_type);
                    body.extend_from_slice(f.value.as_bytes());
                    body.push(0);
                }
                body.push(0);
                msg.extend_from_slice(&((body.len() + 4) as i32).to_be_bytes());
                msg.extend_from_slice(&body);
                msg
            }
            BackendMessage::ParseComplete => {
                let mut msg = vec![b'1'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg
            }
            BackendMessage::BindComplete => {
                let mut msg = vec![b'2'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg
            }
            BackendMessage::CloseComplete => {
                let mut msg = vec![b'3'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg
            }
            BackendMessage::NoData => {
                let mut msg = vec![b'n'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg
            }
            BackendMessage::ParameterDescription { types } => {
                let mut msg = vec![b't'];
                let mut body = Vec::new();
                body.extend_from_slice(&(types.len() as i16).to_be_bytes());
                for t in types {
                    body.extend_from_slice(&t.to_be_bytes());
                }
                msg.extend_from_slice(&((body.len() + 4) as i32).to_be_bytes());
                msg.extend_from_slice(&body);
                msg
            }
            BackendMessage::RowDescriptionEmpty => {
                let mut msg = vec![b'T'];
                msg.extend_from_slice(&(4_i32.to_be_bytes()));
                msg.extend_from_slice(&(0_i16).to_be_bytes());
                msg
            }
        }
    }
}

pub fn encode(messages: &[BackendMessage]) -> Vec<u8> {
    messages.iter().flat_map(|m| m.encode()).collect()
}
