#[derive(Debug, Clone)]
pub enum Message {
    StartupMessage {
        protocol_version: u32,
        parameters: std::collections::HashMap<String, String>,
    },
    Query {
        sql: String,
    },
    Parse {
        name: String,
        sql: String,
        parameter_types: Vec<u32>,
    },
    Bind {
        portal: String,
        statement: String,
        parameter_formats: Vec<i16>,
        parameter_values: Vec<Option<Vec<u8>>>,
        result_formats: Vec<i16>,
    },
    Describe {
        kind: u8, // 'S' for statement, 'P' for portal
        name: String,
    },
    Execute {
        portal: String,
        max_rows: i32,
    },
    Sync,
    Close {
        kind: u8,
        name: String,
    },
    Flush,
    Terminate,
}

impl Message {
    pub fn decode(data: &[u8]) -> anyhow::Result<Vec<Message>> {
        let mut messages = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            if pos + 4 > data.len() {
                break;
            }

            let msg_type = data[pos];
            let length =
                i32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]])
                    as usize;

            if length < 4 || pos + 1 + length > data.len() {
                break;
            }

            let body = &data[pos + 5..pos + 1 + length];
            let msg = match msg_type {
                b'Q' => {
                    let sql = String::from_utf8_lossy(body)
                        .trim_end_matches('\0')
                        .to_string();
                    Message::Query { sql }
                }
                b'P' => {
                    let mut cursor = 0;
                    let name = read_cstring(body, &mut cursor);
                    let sql = read_cstring(body, &mut cursor);
                    let num_params = u16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    let mut parameter_types = Vec::new();
                    for _ in 0..num_params {
                        let oid = u32::from_be_bytes([
                            body[cursor],
                            body[cursor + 1],
                            body[cursor + 2],
                            body[cursor + 3],
                        ]);
                        parameter_types.push(oid);
                        cursor += 4;
                    }
                    Message::Parse {
                        name,
                        sql,
                        parameter_types,
                    }
                }
                b'B' => {
                    let mut cursor = 0;
                    let portal = read_cstring(body, &mut cursor);
                    let statement = read_cstring(body, &mut cursor);
                    let num_formats = u16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    let mut parameter_formats = Vec::new();
                    for _ in 0..num_formats {
                        let fmt = i16::from_be_bytes([body[cursor], body[cursor + 1]]);
                        parameter_formats.push(fmt);
                        cursor += 2;
                    }
                    let num_values = u16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    let mut parameter_values = Vec::new();
                    for _ in 0..num_values {
                        let len = i32::from_be_bytes([
                            body[cursor],
                            body[cursor + 1],
                            body[cursor + 2],
                            body[cursor + 3],
                        ]);
                        cursor += 4;
                        if len == -1 {
                            parameter_values.push(None);
                        } else {
                            let val = body[cursor..cursor + len as usize].to_vec();
                            parameter_values.push(Some(val));
                            cursor += len as usize;
                        }
                    }
                    let num_result_formats =
                        u16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    let mut result_formats = Vec::new();
                    for _ in 0..num_result_formats {
                        let fmt = i16::from_be_bytes([body[cursor], body[cursor + 1]]);
                        result_formats.push(fmt);
                        cursor += 2;
                    }
                    Message::Bind {
                        portal,
                        statement,
                        parameter_formats,
                        parameter_values,
                        result_formats,
                    }
                }
                b'D' => {
                    let kind = body[0];
                    let name = String::from_utf8_lossy(&body[1..])
                        .trim_end_matches('\0')
                        .to_string();
                    Message::Describe { kind, name }
                }
                b'E' => {
                    let mut cursor = 0;
                    let portal = read_cstring(body, &mut cursor);
                    let max_rows = i32::from_be_bytes([
                        body[cursor],
                        body[cursor + 1],
                        body[cursor + 2],
                        body[cursor + 3],
                    ]);
                    Message::Execute { portal, max_rows }
                }
                b'S' => Message::Sync,
                b'C' => {
                    let kind = body[0];
                    let name = String::from_utf8_lossy(&body[1..])
                        .trim_end_matches('\0')
                        .to_string();
                    Message::Close { kind, name }
                }
                b'H' => Message::Flush,
                b'X' => Message::Terminate,
                _ => {
                    pos += 1 + length;
                    continue;
                }
            };

            messages.push(msg);
            pos += 1 + length;
        }

        Ok(messages)
    }
}

fn read_cstring(data: &[u8], cursor: &mut usize) -> String {
    let start = *cursor;
    while *cursor < data.len() && data[*cursor] != 0 {
        *cursor += 1;
    }
    let s = String::from_utf8_lossy(&data[start..*cursor]).to_string();
    if *cursor < data.len() {
        *cursor += 1;
    }
    s
}
