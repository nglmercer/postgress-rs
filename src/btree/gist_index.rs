use crate::types::{Oid, PageId};

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    pub fn contains_box(&self, other: &BoundingBox) -> bool {
        self.min_x <= other.min_x
            && self.max_x >= other.max_x
            && self.min_y <= other.min_y
            && self.max_y >= other.max_y
    }

    pub fn area(&self) -> f64 {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
}

#[derive(Debug, Clone)]
pub struct Range<T: PartialOrd> {
    pub lower: Option<T>,
    pub upper: Option<T>,
    pub lower_inclusive: bool,
    pub upper_inclusive: bool,
}

impl<T: PartialOrd> Range<T> {
    pub fn new(
        lower: Option<T>,
        upper: Option<T>,
        lower_inclusive: bool,
        upper_inclusive: bool,
    ) -> Self {
        Self {
            lower,
            upper,
            lower_inclusive,
            upper_inclusive,
        }
    }

    pub fn contains(&self, value: &T) -> bool {
        let lower_ok = match &self.lower {
            Some(lower) => {
                if self.lower_inclusive {
                    value >= lower
                } else {
                    value > lower
                }
            }
            None => true,
        };

        let upper_ok = match &self.upper {
            Some(upper) => {
                if self.upper_inclusive {
                    value <= upper
                } else {
                    value < upper
                }
            }
            None => true,
        };

        lower_ok && upper_ok
    }

    pub fn overlaps(&self, other: &Range<T>) -> bool {
        let lower_ok = match (&self.lower, &other.upper) {
            (Some(l), Some(u)) => {
                if self.lower_inclusive && other.upper_inclusive {
                    l <= u
                } else {
                    l < u
                }
            }
            _ => true,
        };

        let upper_ok = match (&self.upper, &other.lower) {
            (Some(u), Some(l)) => {
                if self.upper_inclusive && other.lower_inclusive {
                    u >= l
                } else {
                    u > l
                }
            }
            _ => true,
        };

        lower_ok && upper_ok
    }
}

#[derive(Debug, Clone)]
pub enum GiSTEntry {
    Point {
        x: f64,
        y: f64,
        tid: (PageId, u16),
    },
    Box {
        bbox: BoundingBox,
        tid: (PageId, u16),
    },
    Range {
        range: Range<f64>,
        tid: (PageId, u16),
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiSTIndexType {
    Point,
    Box,
    Range,
}

pub struct GiSTIndex {
    pub index_oid: Oid,
    pub rel_oid: Oid,
    pub index_type: GiSTIndexType,
    pub entries: Vec<GiSTEntry>,
}

fn entry_tid(entry: &GiSTEntry) -> (PageId, u16) {
    match entry {
        GiSTEntry::Point { tid, .. } => *tid,
        GiSTEntry::Box { tid, .. } => *tid,
        GiSTEntry::Range { tid, .. } => *tid,
    }
}

impl GiSTIndex {
    pub fn new(index_oid: Oid, rel_oid: Oid, index_type: GiSTIndexType) -> Self {
        Self {
            index_oid,
            rel_oid,
            index_type,
            entries: Vec::new(),
        }
    }

    pub fn insert_point(&mut self, x: f64, y: f64, tid: (PageId, u16)) {
        self.entries.push(GiSTEntry::Point { x, y, tid });
    }

    pub fn insert_box(&mut self, bbox: BoundingBox, tid: (PageId, u16)) {
        self.entries.push(GiSTEntry::Box { bbox, tid });
    }

    pub fn insert_range(&mut self, range: Range<f64>, tid: (PageId, u16)) {
        self.entries.push(GiSTEntry::Range { range, tid });
    }

    pub fn query_point(&self, x: f64, y: f64) -> Vec<(PageId, u16)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                GiSTEntry::Point { x: px, y: py, tid } => {
                    if (*px - x).abs() < f64::EPSILON && (*py - y).abs() < f64::EPSILON {
                        Some(*tid)
                    } else {
                        None
                    }
                }
                GiSTEntry::Box { bbox, tid } => {
                    if bbox.contains_point(x, y) {
                        Some(*tid)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    pub fn query_bbox(&self, query: &BoundingBox) -> Vec<(PageId, u16)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                GiSTEntry::Point { x, y, tid } => {
                    if query.contains_point(*x, *y) {
                        Some(*tid)
                    } else {
                        None
                    }
                }
                GiSTEntry::Box { bbox, tid } => {
                    if bbox.intersects(query) {
                        Some(*tid)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    pub fn query_range(&self, query: &Range<f64>) -> Vec<(PageId, u16)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                GiSTEntry::Range { range, tid } => {
                    if range.overlaps(query) {
                        Some(*tid)
                    } else {
                        None
                    }
                }
                GiSTEntry::Point { x, .. } => {
                    if query.contains(x) {
                        Some(entry_tid(entry))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect()
    }

    pub fn delete(&mut self, tid: &(PageId, u16)) -> bool {
        if let Some(pos) = self.entries.iter().position(|entry| match entry {
            GiSTEntry::Point { tid: t, .. } => t == tid,
            GiSTEntry::Box { tid: t, .. } => t == tid,
            GiSTEntry::Range { tid: t, .. } => t == tid,
        }) {
            self.entries.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn compute_bounding_box(&self) -> Option<BoundingBox> {
        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;
        let mut found = false;

        for entry in &self.entries {
            match entry {
                GiSTEntry::Point { x, y, .. } => {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(*x);
                    max_y = max_y.max(*y);
                    found = true;
                }
                GiSTEntry::Box { bbox, .. } => {
                    min_x = min_x.min(bbox.min_x);
                    min_y = min_y.min(bbox.min_y);
                    max_x = max_x.max(bbox.max_x);
                    max_y = max_y.max(bbox.max_y);
                    found = true;
                }
                _ => {}
            }
        }

        if found {
            Some(BoundingBox::new(min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_contains_point() {
        let bbox = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(bbox.contains_point(5.0, 5.0));
        assert!(bbox.contains_point(0.0, 0.0));
        assert!(bbox.contains_point(10.0, 10.0));
        assert!(!bbox.contains_point(11.0, 5.0));
        assert!(!bbox.contains_point(5.0, 11.0));
    }

    #[test]
    fn test_bounding_box_intersects() {
        let b1 = BoundingBox::new(0.0, 0.0, 10.0, 10.0);
        let b2 = BoundingBox::new(5.0, 5.0, 15.0, 15.0);
        let b3 = BoundingBox::new(20.0, 20.0, 30.0, 30.0);

        assert!(b1.intersects(&b2));
        assert!(b2.intersects(&b1));
        assert!(!b1.intersects(&b3));
    }

    #[test]
    fn test_bounding_box_contains_box() {
        let outer = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let inner = BoundingBox::new(10.0, 10.0, 50.0, 50.0);
        let outside = BoundingBox::new(200.0, 200.0, 300.0, 300.0);

        assert!(outer.contains_box(&inner));
        assert!(!inner.contains_box(&outer));
        assert!(!outer.contains_box(&outside));
    }

    #[test]
    fn test_range_contains() {
        let range = Range::new(Some(1.0), Some(10.0), true, true);
        assert!(range.contains(&5.0));
        assert!(range.contains(&1.0));
        assert!(range.contains(&10.0));
        assert!(!range.contains(&0.5));
        assert!(!range.contains(&10.5));
    }

    #[test]
    fn test_range_overlaps() {
        let r1 = Range::new(Some(1.0), Some(10.0), true, true);
        let r2 = Range::new(Some(5.0), Some(15.0), true, true);
        let r3 = Range::new(Some(20.0), Some(30.0), true, true);

        assert!(r1.overlaps(&r2));
        assert!(r2.overlaps(&r1));
        assert!(!r1.overlaps(&r3));
    }

    #[test]
    fn test_gist_insert_and_query_point() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Point);
        index.insert_point(1.0, 2.0, (PageId(1), 0));
        index.insert_point(3.0, 4.0, (PageId(1), 1));

        let results = index.query_point(1.0, 2.0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], (PageId(1), 0));
    }

    #[test]
    fn test_gist_query_bbox() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Box);
        index.insert_box(BoundingBox::new(0.0, 0.0, 10.0, 10.0), (PageId(1), 0));
        index.insert_box(BoundingBox::new(20.0, 20.0, 30.0, 30.0), (PageId(1), 1));

        let query = BoundingBox::new(5.0, 5.0, 25.0, 25.0);
        let results = index.query_bbox(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_gist_query_range() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Range);
        index.insert_range(
            Range::new(Some(1.0), Some(10.0), true, true),
            (PageId(1), 0),
        );
        index.insert_range(
            Range::new(Some(20.0), Some(30.0), true, true),
            (PageId(1), 1),
        );

        let query = Range::new(Some(5.0), Some(25.0), true, true);
        let results = index.query_range(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_gist_delete() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Point);
        index.insert_point(1.0, 2.0, (PageId(1), 0));

        assert!(index.delete(&(PageId(1), 0)));
        assert_eq!(index.entry_count(), 0);
    }

    #[test]
    fn test_gist_entry_count() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Point);
        index.insert_point(1.0, 2.0, (PageId(1), 0));
        index.insert_point(3.0, 4.0, (PageId(1), 1));

        assert_eq!(index.entry_count(), 2);
    }

    #[test]
    fn test_gist_compute_bounding_box() {
        let mut index = GiSTIndex::new(Oid(1), Oid(100), GiSTIndexType::Point);
        index.insert_point(1.0, 2.0, (PageId(1), 0));
        index.insert_point(5.0, 6.0, (PageId(1), 1));
        index.insert_point(3.0, 4.0, (PageId(1), 2));

        let bbox = index.compute_bounding_box().unwrap();
        assert_eq!(bbox.min_x, 1.0);
        assert_eq!(bbox.min_y, 2.0);
        assert_eq!(bbox.max_x, 5.0);
        assert_eq!(bbox.max_y, 6.0);
    }
}
