// mini rocksdb
// a small key value store i am building to learn rust and how lsm tree works.
// step 3: flush the memtable to an sstable file, and read from files too.

mod db;
mod memtable;
mod sstable;
mod wal;

use db::Db;

fn main() -> std::io::Result<()> {
    let mut database = Db::open("demo_db")?;

    database.put(b"name", b"sparsh")?;
    database.put(b"city", b"pune")?;

    // flush moves the memtable into a sorted file on disk.
    database.flush()?;

    // this key goes to the new memtable, after the flush.
    database.put(b"lang", b"rust")?;
    database.delete(b"city")?;

    for key in [b"name".as_slice(), b"city", b"lang"] {
        match database.get(key)? {
            Some(v) => println!(
                "{} = {}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(&v)
            ),
            None => println!("{} not found", String::from_utf8_lossy(key)),
        }
    }

    Ok(())
}
