use serde::{Deserialize, Serialize};

pub mod jsonb;
pub mod tsvector;
pub mod tsquery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Oid(pub u32);

pub type XID = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct PageId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SlotId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ItemPointerData {
    pub page_id: PageId,
    pub offset: u16,
}

impl std::fmt::Display for ItemPointerData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({},{})", self.page_id.0, self.offset)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tuple {
    pub slots: Vec<SlotId>,
    pub data: Vec<u8>,
    pub xmin: u64,
    pub xmax: u64,
    pub cmin: u32,
    pub cmax: u32,
    pub xvac: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TupleDesc {
    pub fields: Vec<Attribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub type_oid: Oid,
    pub attnum: i16,
    pub typmod: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub rel_oid: Oid,
    pub name: String,
    pub tuple_desc: TupleDesc,
    pub pages: Vec<PageId>,
    #[serde(default)]
    pub relpages: u32,
    #[serde(default)]
    pub reltuples: f64,
    #[serde(default)]
    pub relfrozenxid: u32,
}

impl Relation {
    pub fn empty(name: &str, cols: Vec<(&str, Oid)>) -> Self {
        let tuple_desc = TupleDesc {
            fields: cols
                .into_iter()
                .enumerate()
                .map(|(i, (name, type_oid))| Attribute {
                    name: name.to_string(),
                    type_oid,
                    attnum: i as i16,
                    typmod: -1,
                })
                .collect(),
        };
        Self {
            rel_oid: Oid(0),
            name: name.to_string(),
            tuple_desc,
            pages: vec![],
            relpages: 0,
            reltuples: 0.0,
            relfrozenxid: 0,
        }
    }

    pub fn has_toast_columns(&self) -> bool {
        self.tuple_desc.fields.iter().any(|attr| {
            matches!(attr.type_oid.0, 25 | 1043 | 17 | 3802 | 114 | 1009)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oid_equality() {
        assert_eq!(Oid(1), Oid(1));
        assert_ne!(Oid(1), Oid(2));
    }

    #[test]
    fn test_oid_default() {
        assert_eq!(Oid::default(), Oid(0));
    }

    #[test]
    fn test_page_id_default() {
        assert_eq!(PageId::default(), PageId(0));
    }

    #[test]
    fn test_slot_id_default() {
        assert_eq!(SlotId::default(), SlotId(0));
    }

    #[test]
    fn test_item_pointer_display() {
        let tip = ItemPointerData { page_id: PageId(3), offset: 7 };
        assert_eq!(format!("{}", tip), "(3,7)");
    }

    #[test]
    fn test_relation_empty() {
        let rel = Relation::empty("users", vec![("id", Oid(23)), ("name", Oid(25))]);
        assert_eq!(rel.rel_oid, Oid(0));
        assert_eq!(rel.name, "users");
        assert_eq!(rel.tuple_desc.fields.len(), 2);
        assert_eq!(rel.tuple_desc.fields[0].name, "id");
        assert_eq!(rel.tuple_desc.fields[0].type_oid, Oid(23));
        assert_eq!(rel.tuple_desc.fields[1].name, "name");
        assert!(rel.pages.is_empty());
    }

    #[test]
    fn test_relation_empty_no_columns() {
        let rel = Relation::empty("empty", vec![]);
        assert!(rel.tuple_desc.fields.is_empty());
    }

    #[test]
    fn test_attribute() {
        let attr = Attribute {
            name: "col".to_string(),
            type_oid: Oid(23),
            attnum: 0,
            typmod: -1,
        };
        assert_eq!(attr.name, "col");
        assert_eq!(attr.type_oid, Oid(23));
        assert_eq!(attr.attnum, 0);
    }
}
