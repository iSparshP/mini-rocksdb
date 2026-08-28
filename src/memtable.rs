// memtable
// this is where new writes go first (in memory).
// it keeps keys sorted so later i can write them to a file in order.
// keys and values are just bytes.

use std::collections::BTreeMap;

// each key holds either a real value, or a delete marker (tombstone).
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Value(Vec<u8>),
    Tombstone,
}

// the memtable is just a sorted map from key to entry.
pub struct Memtable {
    map: BTreeMap<Vec<u8>, Entry>,
}

impl Memtable {
    // make an empty memtable.
    pub fn new() -> Self {
        Memtable {
            map: BTreeMap::new(),
        }
    }

    // put a key and value. if the key is already there it is replaced.
    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.map.insert(key.to_vec(), Entry::Value(value.to_vec()));
    }

    // delete a key. i do not remove it, i write a tombstone.
    pub fn delete(&mut self, key: &[u8]) {
        self.map.insert(key.to_vec(), Entry::Tombstone);
    }

    // get the value for a key.
    // if the key is missing, or it was deleted, i return none.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        match self.map.get(key) {
            Some(Entry::Value(v)) => Some(v.clone()),
            Some(Entry::Tombstone) => None,
            None => None,
        }
    }

    // how many keys are in the memtable.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    // is the memtable empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_get() {
        let mut m = Memtable::new();
        m.put(b"name", b"sparsh");
        assert_eq!(m.get(b"name"), Some(b"sparsh".to_vec()));
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
        assert_eq!(m.get(b"k"), Some(b"two".to_vec()));
    }

    #[test]
    fn delete_makes_it_missing() {
        let mut m = Memtable::new();
        m.put(b"k", b"value");
        m.delete(b"k");
        assert_eq!(m.get(b"k"), None);
    }

    #[test]
    fn len_counts_keys() {
        let mut m = Memtable::new();
        assert!(m.is_empty());
        m.put(b"a", b"1");
        m.put(b"b", b"2");
        assert_eq!(m.len(), 2);
    }
}
