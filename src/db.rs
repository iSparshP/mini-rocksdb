// db
// ties the wal and the memtable together.
// every write goes to the wal first (safe on disk), then the memtable (fast reads).
// on open i replay the wal to rebuild the memtable, so data survives a restart.

use std::io;
use std::path::Path;

use crate::memtable::Memtable;
use crate::wal::{Wal, WalRecord};

pub struct Db {
    wal: Wal,
    memtable: Memtable,
}

impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Db> {
        let wal = Wal::open(&path)?;

        let mut memtable = Memtable::new();
        for record in Wal::read_all(&path)? {
            match record {
                WalRecord::Put { key, value } => memtable.put(&key, &value),
                WalRecord::Delete { key } => memtable.delete(&key),
            }
        }

        Ok(Db { wal, memtable })
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.wal.append_put(key, value)?;
        self.memtable.put(key, value);
        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> io::Result<()> {
        self.wal.append_delete(key)?;
        self.memtable.delete(key);
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.memtable.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mini_rocksdb_db_test_{}.log", name));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn put_and_get() {
        let path = temp_path("put_get");
        let mut db = Db::open(&path).unwrap();
        db.put(b"name", b"sparsh").unwrap();
        assert_eq!(db.get(b"name"), Some(b"sparsh".to_vec()));
    }

    #[test]
    fn delete_hides_key() {
        let path = temp_path("delete");
        let mut db = Db::open(&path).unwrap();
        db.put(b"k", b"v").unwrap();
        db.delete(b"k").unwrap();
        assert_eq!(db.get(b"k"), None);
    }

    #[test]
    fn data_survives_reopen() {
        let path = temp_path("reopen");
        {
            let mut db = Db::open(&path).unwrap();
            db.put(b"a", b"1").unwrap();
            db.put(b"b", b"2").unwrap();
        }

        let db = Db::open(&path).unwrap();
        assert_eq!(db.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(db.get(b"b"), Some(b"2".to_vec()));
    }

    #[test]
    fn delete_survives_reopen() {
        let path = temp_path("delete_reopen");
        {
            let mut db = Db::open(&path).unwrap();
            db.put(b"k", b"v").unwrap();
            db.delete(b"k").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get(b"k"), None);
    }

    #[test]
    fn latest_value_wins_after_reopen() {
        let path = temp_path("latest");
        {
            let mut db = Db::open(&path).unwrap();
            db.put(b"k", b"old").unwrap();
            db.put(b"k", b"new").unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.get(b"k"), Some(b"new".to_vec()));
    }
}
