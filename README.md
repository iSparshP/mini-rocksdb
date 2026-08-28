# mini rocksdb

this is my project to build a small key value store in rust.
i am learning rust and i want to understand how rocksdb works inside.

rocksdb uses something called an lsm tree. so i will try to build a small
version of it myself, step by step.

## what it will do

- put a key and value
- get a value by key
- delete a key
- save data to disk so it is not lost
- read data back after restart

## plan (i will do one part at a time)

1. memtable - keep data in memory in sorted order
2. wal - write to a log file first so data is safe if it crashes
3. sstable - write the memtable to a file on disk
4. index - find a key in the file fast
5. bloom filter - skip files that do not have the key
6. get - read from memory and all files, newest wins
7. compaction - merge files and remove old data
8. recovery - read the log file on restart
9. benchmark - test it and compare with real rocksdb

## how to run

```
cargo run
```

## notes

i wrote my learning notes about the concepts in `CONCEPTS.md`.
this is a learning project. it will be slow and simple on purpose.
