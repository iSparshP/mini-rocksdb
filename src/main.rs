// mini rocksdb
// a small key value store i am building to learn rust and how lsm tree works.
// step 2: wal + memtable together in a small db, with recovery.

mod db;
mod memtable;
mod wal;

use db::Db;

fn main() -> std::io::Result<()> {
    // keep the log file next to the program for this small demo.
    let mut database = Db::open("mini_rocksdb.log")?;

    database.put(b"name", b"sparsh")?;
    database.put(b"city", b"pune")?;
    database.delete(b"city")?;

    match database.get(b"name") {
        Some(v) => println!("name = {}", String::from_utf8_lossy(&v)),
        None => println!("name not found"),
    }
    match database.get(b"city") {
        Some(v) => println!("city = {}", String::from_utf8_lossy(&v)),
        None => println!("city not found (deleted)"),
    }

    println!("stop and run again, the data comes back from the wal file.");
    Ok(())
}
