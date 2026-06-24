use serde::{Deserialize, Serialize};

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
        }
    }
}
