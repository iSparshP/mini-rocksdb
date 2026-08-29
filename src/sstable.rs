// sstable (sorted string table)
// an immutable file on disk. i write the whole memtable into it, sorted by key.
// once written it never changes. updates and deletes go into newer files.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::memtable::Entry;

// same record layout as the wal:
//   op(1) key_len(4) key val_len(4) val
// op is 1 for a value, 2 for a tombstone. lengths are little endian u32.
const OP_VALUE: u8 = 1;
const OP_TOMBSTONE: u8 = 2;

pub struct Sstable;

impl Sstable {
    // write all entries to a new file. entries must come in sorted key order.
    pub fn write<'a, P: AsRef<Path>>(
        path: P,
        entries: impl Iterator<Item = (&'a Vec<u8>, &'a Entry)>,
    ) -> io::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        for (key, entry) in entries {
            match entry {
                Entry::Value(value) => {
                    writer.write_all(&[OP_VALUE])?;
                    writer.write_all(&(key.len() as u32).to_le_bytes())?;
                    writer.write_all(key)?;
                    writer.write_all(&(value.len() as u32).to_le_bytes())?;
                    writer.write_all(value)?;
                }
                Entry::Tombstone => {
                    writer.write_all(&[OP_TOMBSTONE])?;
                    writer.write_all(&(key.len() as u32).to_le_bytes())?;
                    writer.write_all(key)?;
                    writer.write_all(&0u32.to_le_bytes())?;
                }
            }
        }

        writer.flush()?;
        Ok(())
    }

    // look for a key in this file.
    // returns the entry (value or tombstone) if the key is here, else none.
    // the file is sorted, so i can stop early once i pass the key.
    pub fn get<P: AsRef<Path>>(path: P, target: &[u8]) -> io::Result<Option<Entry>> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

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

            if key.as_slice() == target {
                let entry = match op[0] {
                    OP_VALUE => Entry::Value(value),
                    OP_TOMBSTONE => Entry::Tombstone,
                    other => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("bad op byte {}", other),
                        ))
                    }
                };
                return Ok(Some(entry));
            }

            // keys are sorted, so if we went past the target it is not here.
            if key.as_slice() > target {
                break;
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memtable::Memtable;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("mini_rocksdb_sst_test_{}.sst", name));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn write_then_get_value() {
        let path = temp_path("value");
        let mut m = Memtable::new();
        m.put(b"a", b"1");
        m.put(b"b", b"2");
        Sstable::write(&path, m.entries()).unwrap();

        assert_eq!(
            Sstable::get(&path, b"a").unwrap(),
            Some(Entry::Value(b"1".to_vec()))
        );
        assert_eq!(
            Sstable::get(&path, b"b").unwrap(),
            Some(Entry::Value(b"2".to_vec()))
        );
    }

    #[test]
    fn missing_key_returns_none() {
        let path = temp_path("missing");
        let mut m = Memtable::new();
        m.put(b"a", b"1");
        Sstable::write(&path, m.entries()).unwrap();

        assert_eq!(Sstable::get(&path, b"zzz").unwrap(), None);
    }

    #[test]
    fn tombstone_is_stored() {
        let path = temp_path("tombstone");
        let mut m = Memtable::new();
        m.put(b"a", b"1");
        m.delete(b"a");
        Sstable::write(&path, m.entries()).unwrap();

        assert_eq!(Sstable::get(&path, b"a").unwrap(), Some(Entry::Tombstone));
    }
}
