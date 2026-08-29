// wal (write ahead log)
// before a write goes to the memtable, i first append it to this log file on disk.
// if the program crashes, i can read this file again and rebuild the memtable.
// that is called replay.
//
// YOUR JOB: fill in the three todo!() methods. open() is done for you as an example.
// run `cargo test wal` to check your work. it fails until you implement them.

use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

// one thing i wrote to the log: either a put (key + value) or a delete (key only).
#[derive(Debug, Clone, PartialEq)]
pub enum WalRecord {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

// the byte layout of one record on disk (i keep it the same for both kinds):
//
//   [ op       : 1 byte  ]   1 = put, 2 = delete
//   [ key_len  : 4 bytes ]   little endian u32
//   [ key      : key_len bytes ]
//   [ val_len  : 4 bytes ]   little endian u32  (0 for a delete)
//   [ val      : val_len bytes ]                 (empty for a delete)

const OP_PUT: u8 = 1;
const OP_DELETE: u8 = 2;

pub struct Wal {
    file: File,
}

impl Wal {
    // open the log file for appending. create it if it does not exist.
    // this one is done for you. read it to see how OpenOptions works.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Wal> {
        let file = OpenOptions::new()
            .create(true) // make the file if it is not there
            .append(true) // always write at the end, never overwrite
            .read(true)
            .open(path)?;
        Ok(Wal { file })
    }

    // append a put record to the log.
    //
    // steps:
    //  1. write one byte: OP_PUT
    //  2. write key length as 4 bytes: (key.len() as u32).to_le_bytes()
    //  3. write the key bytes
    //  4. write value length as 4 bytes
    //  5. write the value bytes
    //  6. flush so it really reaches the disk
    // use self.file.write_all(...) for each piece, and self.file.flush() at the end.
    // return Ok(()) when done. use the ? operator to pass errors up.
    pub fn append_put(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        // todo!("write op, key_len, key, val_len, val, then flush")
        self.file.write_all(&[OP_PUT])?;
        self.file.write_all(&(key.len() as u32).to_le_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(&(value.len() as u32).to_le_bytes())?;
        self.file.write_all(value)?;
        self.file.flush()?;
        Ok(())
    }

    // append a delete record. same as put but op is OP_DELETE and there is no value,
    // so val_len is 0 and you write no value bytes.
    pub fn append_delete(&mut self, key: &[u8]) -> io::Result<()> {
        // todo!("write OP_DELETE, key_len, key, and a val_len of 0")
        self.file.write_all(&[OP_DELETE])?;
        self.file.write_all(&(key.len() as u32).to_le_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(&0u32.to_le_bytes())?; // val_len = 0
        self.file.flush()?;
        Ok(())
    }

    // read the whole log back from the start and return every record in order.
    // this is the replay step.
    //
    // steps:
    //  1. open the file at `path` and wrap it in a BufReader
    //  2. loop:
    //       read 1 byte for op. if you hit end of file here, stop and return what you have.
    //       read 4 bytes for key_len, turn into u32 with u32::from_le_bytes
    //       read that many bytes for the key
    //       read 4 bytes for val_len, then that many bytes for the value
    //       build a WalRecord::Put or WalRecord::Delete based on op and push it
    //
    // helpers you will want:
    //   let mut one = [0u8; 1];  reader.read_exact(&mut one)?;   // read exactly 1 byte
    //   let mut four = [0u8; 4]; reader.read_exact(&mut four)?;  // read exactly 4 bytes
    //   let n = u32::from_le_bytes(four) as usize;
    //   let mut buf = vec![0u8; n]; reader.read_exact(&mut buf)?;// read n bytes
    //
    // to detect end of file cleanly on the FIRST read of a record, check the result:
    //   match reader.read_exact(&mut one) {
    //       Ok(()) => { /* got a record, keep going */ }
    //       Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
    //       Err(e) => return Err(e),
    //   }
    pub fn read_all<P: AsRef<Path>>(path: P) -> io::Result<Vec<WalRecord>> {
        // todo!("open the file, loop reading records until end of file, return the vec")
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();

        loop {
            let mut op = [0u8; 1];
            match reader.read_exact(&mut op) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break, // clean end
                Err(e) => return Err(e),
            }

            // read key
            let mut four = [0u8; 4];
            reader.read_exact(&mut four)?;
            let key_len = u32::from_le_bytes(four) as usize;
            let mut key = vec![0u8; key_len];
            reader.read_exact(&mut key)?;

            // read value
            reader.read_exact(&mut four)?;
            let val_len = u32::from_le_bytes(four) as usize;
            let mut value = vec![0u8; val_len];
            reader.read_exact(&mut value)?;

            // build the record based on op
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

    // give each test its own file so they do not clash.
    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mini_rocksdb_wal_test_{}.log", name));
        let _ = std::fs::remove_file(&p); // start clean
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
        // open again like after a restart, add more
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
            let _wal = Wal::open(&path).unwrap(); // just create it
        }
        let records = Wal::read_all(&path).unwrap();
        assert!(records.is_empty());
    }
}
