// mini rocksdb
// a small key value store i am building to learn rust and how lsm tree works.
// step 1: the memtable (writes go here first, in memory).

mod memtable;
mod wal;

use memtable::Memtable;

fn main() {
    let mut m = Memtable::new();

    m.put(b"name", b"sparsh");
    m.put(b"city", b"pune");

    // read a key back
    match m.get(b"name") {
        Some(v) => println!("name = {}", String::from_utf8_lossy(&v)),
        None => println!("name not found"),
    }

    // delete a key and read again
    m.delete(b"city");
    match m.get(b"city") {
        Some(v) => println!("city = {}", String::from_utf8_lossy(&v)),
        None => println!("city not found (deleted)"),
    }

    println!("keys in memtable: {}", m.len());
}
