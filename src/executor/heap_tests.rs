#[cfg(test)]
mod tests {
    use crate::executor::heap::{decode_tuple_values, is_visible, Filter};
    use crate::transaction::{Snapshot, TransactionId};
    use crate::types::{Attribute, Oid, SlotId, Tuple, TupleDesc};

    fn make_test_tuple(values: Vec<&str>) -> Tuple {
        let data: Vec<u8> = values
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                let mut bytes = s.as_bytes().to_vec();
                if i > 0 {
                    bytes.insert(0, 0);
                }
                bytes
            })
            .collect();
        Tuple {
            slots: vec![SlotId(0)],
            data,
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        }
    }

    #[test]
    fn test_tuple_serialization_roundtrip() {
        let tuple = make_test_tuple(vec!["hello", "world"]);
        let encoded = bincode::serialize(&tuple).unwrap();
        let decoded: Tuple = bincode::deserialize(&encoded).unwrap();
        assert_eq!(tuple.data, decoded.data);
        assert_eq!(tuple.xmin, decoded.xmin);
    }

    #[test]
    fn test_tuple_with_empty_data() {
        let tuple = Tuple {
            slots: vec![],
            data: vec![],
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let encoded = bincode::serialize(&tuple).unwrap();
        let decoded: Tuple = bincode::deserialize(&encoded).unwrap();
        assert!(decoded.data.is_empty());
    }

    #[test]
    fn test_tuple_visibility_not_visible() {
        let tup = Tuple {
            slots: vec![SlotId(0)],
            data: vec![1, 0, 2],
            xmin: 100,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let snapshot = Snapshot {
            xid: TransactionId(50),
            active_xids: vec![],
        };
        assert!(!is_visible(&tup, &snapshot));
    }

    #[test]
    fn test_tuple_visibility_expired_xmin() {
        let tup = Tuple {
            slots: vec![SlotId(0)],
            data: vec![1],
            xmin: 10,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let snapshot = Snapshot {
            xid: TransactionId(100),
            active_xids: vec![],
        };
        assert!(is_visible(&tup, &snapshot));
    }

    #[test]
    fn test_tuple_visibility_with_xmax() {
        let tup = Tuple {
            slots: vec![SlotId(0)],
            data: vec![1],
            xmin: 10,
            xmax: 50,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let snapshot = Snapshot {
            xid: TransactionId(100),
            active_xids: vec![],
        };
        assert!(!is_visible(&tup, &snapshot));
    }

    #[test]
    fn test_filter_creation() {
        let filter = Filter {
            column: 0,
            value: b"test".to_vec(),
        };
        assert_eq!(filter.column, 0);
        assert_eq!(filter.value, b"test");
    }

    #[test]
    fn test_slow_scan_filter_matching() {
        let row = ["alice".to_string(), "30".to_string()];
        let filter = Filter {
            column: 0,
            value: b"alice".to_vec(),
        };
        let filter_col = filter.column;
        let expected = String::from_utf8_lossy(&filter.value);
        let matches = row[filter_col].contains(&*expected) || row[filter_col] == expected;
        assert!(matches);
    }

    #[test]
    fn test_slow_scan_filter_not_matching() {
        let row = ["bob".to_string(), "25".to_string()];
        let filter = Filter {
            column: 0,
            value: b"alice".to_vec(),
        };
        let filter_col = filter.column;
        let expected = String::from_utf8_lossy(&filter.value);
        let matches = row[filter_col].contains(&*expected) || row[filter_col] == expected;
        assert!(!matches);
    }

    #[test]
    fn test_decode_tuple_values_single_column() {
        let desc = TupleDesc {
            fields: vec![Attribute {
                name: "name".to_string(),
                type_oid: Oid(25),
                attnum: 0,
                typmod: -1,
            }],
        };
        let tup = Tuple {
            slots: vec![SlotId(0)],
            data: b"alice".to_vec(),
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let values = decode_tuple_values(&tup, &desc);
        assert_eq!(values, vec!["alice"]);
    }

    #[test]
    fn test_decode_tuple_values_multiple_columns() {
        let desc = TupleDesc {
            fields: vec![
                Attribute {
                    name: "id".to_string(),
                    type_oid: Oid(23),
                    attnum: 0,
                    typmod: -1,
                },
                Attribute {
                    name: "name".to_string(),
                    type_oid: Oid(25),
                    attnum: 1,
                    typmod: -1,
                },
            ],
        };
        let tup = Tuple {
            slots: vec![SlotId(0), SlotId(1)],
            data: b"1\0alice".to_vec(),
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let values = decode_tuple_values(&tup, &desc);
        assert_eq!(values, vec!["1", "alice"]);
    }

    #[test]
    fn test_decode_tuple_values_empty() {
        let desc = TupleDesc { fields: vec![] };
        let tup = Tuple {
            slots: vec![],
            data: vec![],
            xmin: 1,
            xmax: 0,
            cmin: 0,
            cmax: 0,
            xvac: 0,
        };
        let values = decode_tuple_values(&tup, &desc);
        assert!(values.is_empty());
    }
}
