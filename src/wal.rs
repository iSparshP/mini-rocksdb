// wal (write ahead log)
// every write is appended here first, so it is safe on disk.
// on start i read this file back to rebuild the memtable. that is replay.

use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum WalRecord {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

// record layout on disk:
//   op(1) key_len(4) key val_len(4) val
// op is 1 for put, 2 for delete. lengths are little endian u32. delete has val_len 0.
const OP_PUT: u8 = 1;
const OP_DELETE: u8 = 2;

pub struct Wal {
    file: File,
}

impl Wal {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Wal> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;
        Ok(Wal { file })
    }

    pub fn append_put(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.file.write_all(&[OP_PUT])?;
        self.file.write_all(&(key.len() as u32).to_le_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(&(value.len() as u32).to_le_bytes())?;
        self.file.write_all(value)?;
        self.file.flush()?;
        Ok(())
    }

    pub fn append_delete(&mut self, key: &[u8]) -> io::Result<()> {
        self.file.write_all(&[OP_DELETE])?;
        self.file.write_all(&(key.len() as u32).to_le_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(&0u32.to_le_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    // empty the log. i call this after a flush, when the data is safe in an sstable.
    pub fn clear(&mut self) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.flush()?;
        Ok(())
    }

    pub fn read_all<P: AsRef<Path>>(path: P) -> io::Result<Vec<WalRecord>> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();

        loop {
            let mut op = [0u8; 1];
            match reader.read_exact(&mut op) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let mut four = [0u8; 4];
            reader.read_exact(&mut four)?;
            let key_len = u32::from_le_bytes(four) as usize;
            let mut key = vec![0u8; key_len];
            reader.read_exact(&mut key)?;

            reader.read_exact(&mut four)?;
            let val_len = u32::from_le_bytes(four) as usize;
            let mut value = vec![0u8; val_len];
            reader.read_exact(&mut value)?;

            let record = match op[0] {
                OP_PUT => WalRecord::Put { key, value },
                OP_DELETE => WalRecord::Delete { key },
                other => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("bad op byte {}", other),
                    ))
                }
            };
            records.push(record);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mini_rocksdb_wal_test_{}.log", name));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn put_then_replay() {
        let path = temp_path("put");
        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append_put(b"name", b"sparsh").unwrap();
        }
        let records = Wal::read_all(&path).unwrap();
        assert_eq!(
            records,
            vec![WalRecord::Put {
                key: b"name".to_vec(),
                value: b"sparsh".to_vec()
            }]
        );
    }

    #[test]
    fn delete_then_replay() {
        let path = temp_path("delete");
        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append_delete(b"city").unwrap();
        }
        let records = Wal::read_all(&path).unwrap();
        assert_eq!(
            records,
            vec![WalRecord::Delete {
                key: b"city".to_vec()
            }]
        );
    }

    #[test]
    fn many_records_in_order() {
        let path = temp_path("many");
        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append_put(b"a", b"1").unwrap();
            wal.append_put(b"b", b"22").unwrap();
            wal.append_delete(b"a").unwrap();
        }
        let records = Wal::read_all(&path).unwrap();
        assert_eq!(
            records,
            vec![
                WalRecord::Put {
                    key: b"a".to_vec(),
                    value: b"1".to_vec()
                },
                WalRecord::Put {
                    key: b"b".to_vec(),
                    value: b"22".to_vec()
                },
                WalRecord::Delete { key: b"a".to_vec() },
            ]
        );
    }

    #[test]
    fn reopen_appends_not_overwrites() {
        let path = temp_path("reopen");
        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append_put(b"first", b"1").unwrap();
        }
        {
            let mut wal = Wal::open(&path).unwrap();
            wal.append_put(b"second", b"2").unwrap();
        }
        let records = Wal::read_all(&path).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn empty_log_replays_to_nothing() {
        let path = temp_path("empty");
        {
            let _wal = Wal::open(&path).unwrap();
        }
        let records = Wal::read_all(&path).unwrap();
        assert!(records.is_empty());
    }
}
