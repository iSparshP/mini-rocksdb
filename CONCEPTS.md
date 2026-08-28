# my notes on how mini rocksdb works

these are my learning notes. i want to understand how rocksdb works inside so i can
build a small version of it myself. rocksdb uses an lsm tree. many databases like
leveldb, cassandra and scylla also use the lsm tree idea. so if i learn this one thing
i understand a lot of databases at once.

## the problem this solves

a key value store needs to do put, get and delete. it also should scan keys in order.
the hard part is doing all this fast on disk when the data is bigger than memory.

there are two ways to keep data on disk.

the first way is a b tree. postgres and mysql use this. it keeps keys sorted in pages
and it updates them in place. reading is fast. but writing is slow because every write
becomes a random write on disk. random writes are the slowest thing a disk does.

the second way is the lsm tree. rocksdb uses this. it never updates in place. it keeps
new writes in memory first, and then writes them out as new files that never change.
so writes become simple appends at the end. this is much faster.

the trade off is simple. the lsm tree makes writes fast but reads slower, because one
key can be in many files. so it is a good choice when you write a lot, like logs,
metrics and events. that is the kind of thing i want to build later.

one line to remember. an lsm tree just keeps adding data, and then cleans itself up
later.

## the parts of the engine

when i call put with a key and value, this is what happens.

first i write the change to a log file on disk. this is the wal. it keeps my data safe
if the program crashes.

second i put the change into the memtable. this is a sorted table that lives in memory.

when the memtable gets full, i write it out to a file on disk. this file is called an
sstable. this step is called a flush.

in the background a job called compaction merges the small files into bigger and
cleaner files.

so the flow is like this.

```
put key value
   goes to wal file first (safe on disk)
   then goes to memtable (sorted, in memory)
   when memtable is full it is flushed to an sstable file
   later compaction merges the files
```

## the wal (write ahead log)

the memtable lives in memory. if the program crashes the memtable is gone and i lose
the new writes.

to fix this i write every change to a log file first, before i touch the memtable, and
i make sure it is really saved on disk. now the write is safe. when the program starts
again i read the log file and build the memtable back. this is called replay.

the log file is only appended at the end so it is cheap. when the memtable is flushed to
an sstable, the data is safe in the file, so the old log can be deleted.

## the memtable

this is where new writes go after the wal. every put and delete goes here.

it is sorted by key. i keep it sorted so that when i flush it, the file is already
sorted, and so range scans work. in rust a btree map keeps keys sorted for me.

for each key it stores either a value, or a delete marker. that delete marker is called
a tombstone. i will explain it below.

the memtable has a size limit. when it is full it becomes read only and gets flushed,
and a new empty memtable takes the new writes.

## the sstable (sorted file on disk)

when the memtable is full i write all its data into one file. this file is sorted by
key and it never changes after that. this is important. i never edit an old file. if a
key is updated or deleted, that just goes into a newer file.

inside the file i keep, in sorted order, the key and value pairs, a small index, a bloom
filter, and a footer at the end that tells me where the index and filter are.

the index is a sparse index. it does not store every key. it stores every few keys with
their position in the file. to find a key i search the index to get close, then i scan a
small part of the file. a sparse index is small so it can stay in memory even when the
data can not.

because the files are sorted and never change, merging two of them is easy. it is just
like merging two sorted lists. merging many of them uses a heap.

## flush and compaction

flush is when the memtable in memory is written to a new sstable file. this file goes to
level zero.

compaction is a background job that merges sstable files into fewer and bigger files. i
need it for two reasons. first, if a key can be in many files, reads get slow, so i want
fewer files. second, old values and delete markers pile up and waste space, so
compaction throws them away.

levels work like this. level zero files come straight from flushes, so they can have
keys that overlap each other. that means a read may have to check all level zero files.
lower levels keep files that do not overlap inside the level, and each level is about
ten times bigger than the one above it. no overlap means for one key there is at most
one file per level to check, so reads are fast.

there are three costs i will measure. these are called amplification.

write amplification is when one write gets rewritten many times as it moves down the
levels.

read amplification is when one get has to touch many files.

space amplification is when dead data stays around until compaction removes it.

i can not make all three small at the same time. choosing which one to give up is the
whole game. measuring this is what makes a good blog post.

## the bloom filter

this is the trick that makes reads fast. a bloom filter is a small set of bits that can
answer one question. is this key in this file. it answers in two ways. it can say the
key is definitely not here, so i skip the whole file and read nothing from disk. or it
can say the key is maybe here, so i go and check.

it never says no when the answer is yes. it only sometimes says maybe when the real
answer is no. so it safely skips most files that do not have the key. it is very small
and it saves a lot of disk reads, mostly for single key lookups.

## tombstones (how delete works)

i can not erase a key from a file that never changes. so a delete just writes a marker
that says this key is dead from now on. this marker is the tombstone.

when i read a key, the newest version wins. if the newest version is a tombstone i
answer not found, even if an older file still has a value. later during compaction, once
the tombstone is older than every value for that key, both are removed and the space is
freed.

this is why get returns an optional value. not found can mean the key never existed, or
that it was deleted.

## the read path

this is how get works, all parts together.

```
get key
   check the memtable first
   then check level zero files, newest first
   then check level one, level two and so on
   the newest version of the key wins
   if it is not found anywhere, answer not found
```

for each file, the bloom filter is checked first. if it says maybe, then i search the
index, then read a small block. every trick here exists to make this read touch as
little disk as possible. that is the whole point of a storage engine.

## recovery (safe after a crash)

when the program starts again it does this. it finds the sstable files that already
exist. it finds the log files for any memtable that was not flushed yet. it replays the
log to build the memtable back. now the state is the same as before the crash. and i
never had to do a slow random write to get this safety.

## what i will build and what i will skip

i will build these first.

- keys and values as bytes
- the wal, with append and replay
- the memtable using a btree map, with value or tombstone
- writing and reading an sstable, with a sparse index and a footer
- a bloom filter for each sstable
- flush from memtable to a level zero file
- simple compaction, first just merge all level zero files into one
- get across the memtable and all files, newest wins
- recovery by replaying the log
- a benchmark to test it and compare with real rocksdb

i will skip these for now and add them only if a later project needs them.

- snapshots and multi version reads
- column families and transactions
- compression, i will add this later as a benchmark option
- anything distributed

the idea is not to replace rocksdb. the idea is to build the core of the lsm tree, measure
the costs, and find out exactly where the real rocksdb needs its extra parts.

## the build order

i will do one part at a time. each part should build, run, and have a test.

```
1. memtable in memory
2. wal append and replay, so put get delete are safe
3. write the memtable to an sstable file, and read it back
4. add the sparse index and footer, to find a key fast
5. add the bloom filter, to skip files with a miss
6. get across the memtable and all files, newest wins
7. compaction, merge files and drop old values and tombstones
8. recovery, replay the log on start
9. benchmark, make graphs, compare with real rocksdb
```

by step six i will have a working key value store that is safe on disk. steps seven to
nine make it good and give me the results to write about.
