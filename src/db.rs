// db
// keeps everything together: the wal, the memtable, and the sstable files.
//
// write path:  wal first (safe on disk), then memtable (fast in memory).
// flush:       write the memtable to a new sstable file, clear the wal and memtable.
// read path:   check the memtable first, then the sstables newest to oldest.
//              the newest version of a key wins. a tombstone means "deleted".

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::memtable::{Entry, Memtable};
use crate::sstable::Sstable;
use crate::wal::{Wal, WalRecord};

pub struct Db {
    dir: PathBuf,
    wal: Wal,
    memtable: Memtable,
    sstables: Vec<PathBuf>, // oldest first, newest last
}

impl Db {
    pub fn open<P: AsRef<Path>>(dir: P) -> io::Result<Db> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;

        // find sstable files already on disk, sorted by name (oldest first).
        let mut sstables: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "sst").unwrap_or(false))
            .collect();
        sstables.sort();

        // rebuild the memtable from the wal.
        let wal_path = dir.join("wal.log");
        let wal = Wal::open(&wal_path)?;
        let mut memtable = Memtable::new();
        for record in Wal::read_all(&wal_path)? {
            match record {
                WalRecord::Put { key, value } => memtable.put(&key, &value),
                WalRecord::Delete { key } => memtable.delete(&key),
            }
        }

        Ok(Db {
            dir,
            wal,
            memtable,
            sstables,
        })
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

    pub fn get(&self, key: &[u8]) -> io::Result<Option<Vec<u8>>> {
        // memtable is newest, check it first.
        match self.memtable.get(key) {
            Some(Entry::Value(v)) => return Ok(Some(v.clone())),
            Some(Entry::Tombstone) => return Ok(None),
            None => {}
        }

        // then the sstables, newest to oldest.
        for path in self.sstables.iter().rev() {
            match Sstable::get(path, key)? {
                Some(Entry::Value(v)) => return Ok(Some(v)),
                Some(Entry::Tombstone) => return Ok(None),
                None => {}
            }
        }

        Ok(None)
    }

    // write the memtable to a new sstable file, then clear the wal and memtable.
    pub fn flush(&mut self) -> io::Result<()> {
        let path = self.dir.join(format!("{:06}.sst", self.sstables.len() + 1));
        Sstable::write(&path, self.memtable.entries())?;
        self.wal.clear()?;
        self.memtable = Memtable::new();
        self.sstables.push(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mini_rocksdb_db_test_{}", name));
        let _ = fs::remove_dir_all(&p); // start clean
        p
    }

    #[test]
    fn put_and_get() {
        let dir = temp_dir("put_get");
        let mut db = Db::open(&dir).unwrap();
        db.put(b"name", b"sparsh").unwrap();
        assert_eq!(db.get(b"name").unwrap(), Some(b"sparsh".to_vec()));
    }

    #[test]
    fn delete_hides_key() {
        let dir = temp_dir("delete");
        let mut db = Db::open(&dir).unwrap();
        db.put(b"k", b"v").unwrap();
        db.delete(b"k").unwrap();
        assert_eq!(db.get(b"k").unwrap(), None);
    }

    #[test]
    fn data_survives_reopen() {
        let dir = temp_dir("reopen");
        {
            let mut db = Db::open(&dir).unwrap();
            db.put(b"a", b"1").unwrap();
            db.put(b"b", b"2").unwrap();
        }
        let db = Db::open(&dir).unwrap();
        assert_eq!(db.get(b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(db.get(b"b").unwrap(), Some(b"2".to_vec()));
    }

    // after a flush the data lives in an sstable, not the memtable or wal.
    #[test]
    fn get_reads_from_sstable_after_flush() {
        let dir = temp_dir("flush_get");
        let mut db = Db::open(&dir).unwrap();
        db.put(b"a", b"1").unwrap();
        db.put(b"b", b"2").unwrap();
        db.flush().unwrap();

        // memtable is empty now, these come from the sstable file.
        assert_eq!(db.get(b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(db.get(b"b").unwrap(), Some(b"2".to_vec()));
    }

    // a newer memtable value must win over an older value in an sstable.
    #[test]
    fn newer_value_wins_over_sstable() {
        let dir = temp_dir("newer_wins");
        let mut db = Db::open(&dir).unwrap();
        db.put(b"k", b"old").unwrap();
        db.flush().unwrap();
        db.put(b"k", b"new").unwrap();
        assert_eq!(db.get(b"k").unwrap(), Some(b"new".to_vec()));
    }

    // a delete in the memtable must hide a value that is still in an sstable.
    #[test]
    fn delete_hides_sstable_value() {
        let dir = temp_dir("delete_sst");
        let mut db = Db::open(&dir).unwrap();
        db.put(b"k", b"v").unwrap();
        db.flush().unwrap();
        db.delete(b"k").unwrap();
        assert_eq!(db.get(b"k").unwrap(), None);
    }

    // sstables survive a restart.
    #[test]
    fn sstable_data_survives_reopen() {
        let dir = temp_dir("sst_reopen");
        {
            let mut db = Db::open(&dir).unwrap();
            db.put(b"a", b"1").unwrap();
            db.flush().unwrap();
        }
        let db = Db::open(&dir).unwrap();
        assert_eq!(db.get(b"a").unwrap(), Some(b"1".to_vec()));
    }
}
