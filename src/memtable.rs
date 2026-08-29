// memtable
// where new writes go first, in memory. keys are kept sorted.
// keys and values are just bytes.

use std::collections::BTreeMap;

// each key holds either a real value, or a delete marker (tombstone).
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Value(Vec<u8>),
    Tombstone,
}

pub struct Memtable {
    map: BTreeMap<Vec<u8>, Entry>,
}

impl Memtable {
    pub fn new() -> Self {
        Memtable {
            map: BTreeMap::new(),
        }
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.map.insert(key.to_vec(), Entry::Value(value.to_vec()));
    }

    // delete does not remove the key, it writes a tombstone.
    pub fn delete(&mut self, key: &[u8]) {
        self.map.insert(key.to_vec(), Entry::Tombstone);
    }

    // returns the raw entry so the caller can tell a tombstone from a missing key.
    pub fn get(&self, key: &[u8]) -> Option<&Entry> {
        self.map.get(key)
    }

    // all entries in sorted key order, used to write an sstable.
    pub fn entries(&self) -> impl Iterator<Item = (&Vec<u8>, &Entry)> {
        self.map.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_get() {
        let mut m = Memtable::new();
        m.put(b"name", b"sparsh");
        assert_eq!(m.get(b"name"), Some(&Entry::Value(b"sparsh".to_vec())));
    }

    #[test]
    fn get_missing_key() {
        let m = Memtable::new();
        assert_eq!(m.get(b"nope"), None);
    }

    #[test]
    fn update_replaces_value() {
        let mut m = Memtable::new();
        m.put(b"k", b"one");
        m.put(b"k", b"two");
        assert_eq!(m.get(b"k"), Some(&Entry::Value(b"two".to_vec())));
    }

    #[test]
    fn delete_writes_tombstone() {
        let mut m = Memtable::new();
        m.put(b"k", b"value");
        m.delete(b"k");
        assert_eq!(m.get(b"k"), Some(&Entry::Tombstone));
    }
}
