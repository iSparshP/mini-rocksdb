# Mini RocksDB — The Concepts (read before building)

Goal: understand an **LSM-tree** key-value store deeply enough to rebuild a focused
version in Rust, benchmark it against real RocksDB, and explain the trade-offs.

RocksDB (and LevelDB, Cassandra, ScyllaDB, TiKV, CockroachDB's storage, and the storage
under many databases) is built on the **LSM-tree** (Log-Structured Merge-tree). Learn LSM
= understand all of them.

---

## 0. The problem LSM solves

A key-value store must support: `put(key, value)`, `get(key)`, `delete(key)`, and ideally
ordered range scans. The hard part is doing this **fast on disk** when data is bigger than
RAM.

Two ways to lay data on disk:

**B-tree (Postgres, MySQL/InnoDB):** keeps keys sorted in fixed pages, updated **in place**.
- Read: great — walk the tree, O(log n), few page reads.
- Write: bad — every write is a **random** disk write (find the page, modify, write back).
  Random writes are the slowest thing a disk does.

**LSM-tree (RocksDB):** never updates in place. Buffers writes in memory, then flushes them
as **new immutable sorted files**. Writes become **sequential** appends.
- Write: great — sequential writes are 100–1000× faster than random on HDD, and much
  kinder to SSD (less write wear, better throughput).
- Read: worse — a key might live in any of several files; you may check many.

**The core trade-off in one line:** LSM trades read cost for write cost. It's the right
choice for **write-heavy** workloads — logs, metrics, event streams, time-series. Exactly
your roadmap's territory.

> Key mental model: **an LSM is an append-only system that periodically tidies itself up.**
> Writes append. Cleanup (compaction) merges and discards. That's the whole philosophy.

---

## 1. The moving parts (the whole engine in one picture)

```
        put(k,v) / delete(k)
              │
              ▼
   ┌──────────────────────┐   append first, for durability
   │  WAL (write-ahead log)│──────────────►  disk (sequential append)
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐   sorted, in RAM, fast
   │  Memtable (BTreeMap)  │
   └──────────────────────┘
              │  when full (e.g. 4 MB)  → "flush"
              ▼
   ┌──────────────────────┐   immutable, sorted, on disk
   │   SSTable (L0 file)   │   + Bloom filter + sparse index
   └──────────────────────┘
              │  background "compaction" merges files
              ▼
   L1, L2, ... larger, fewer, non-overlapping SSTables
```

Read path walks these **newest → oldest**:
```
get(k):  Memtable  →  L0 SSTables (newest first)  →  L1  →  L2 ...
         first hit wins (newest version of the key)
```

---

## 2. WAL — Write-Ahead Log (durability)

**Problem:** the Memtable lives in RAM. If the process crashes, unflushed writes vanish.

**Fix:** before touching the Memtable, **append the operation to a log file on disk** and
`fsync` it. Now the write survives a crash. On restart, **replay** the WAL to rebuild the
Memtable exactly as it was.

- Append-only, sequential → cheap.
- One WAL per Memtable. When the Memtable flushes to an SSTable (now safely on disk), its
  WAL can be deleted — the data is durable elsewhere.
- Record format (you'll design it): `[op:1][key_len:4][key][val_len:4][val]` as raw bytes.

This is the same WAL idea in Postgres, Kafka, and etcd. Learn it once.

**Rust you'll use:** `File`, `BufWriter`, `write_all`, `flush`/`sync_all`, byte encoding
(`to_le_bytes`). → your LeetCode `lc08` (parsing) and Lesson 02 (`Result`/`?`) pay off here.

---

## 3. Memtable — in-memory sorted buffer

The live write buffer. Every `put`/`delete` lands here (after the WAL append).

- **Sorted by key** so that flushing produces a sorted file and range scans work. In Rust:
  `BTreeMap<Vec<u8>, Entry>` gives you sorted-by-key for free.
- Stores an **`Entry`** per key: either `Value(bytes)` or `Tombstone` (a delete marker).
- Has a size limit (say 4 MB). When exceeded → becomes immutable and gets flushed; a fresh
  Memtable takes new writes. (Real RocksDB keeps the old one readable while flushing.)

You already built a toy version of this in **Lesson 03**. Same shape.

---

## 4. SSTable — Sorted String Table (the on-disk file)

When a Memtable is full, its sorted contents are written out as one **immutable** file: the
SSTable. "Immutable" is the magic word — once written, never modified. Updates and deletes
are handled by writing *newer* SSTables, not editing old ones.

An SSTable holds, in key-sorted order:
- **Data block:** the actual `key → Entry` pairs, sorted.
- **Sparse index:** not every key — every Nth key with its byte offset. To find a key, binary
  search the sparse index to get "somewhere near here," then scan a small block. (Sparse =
  index fits in RAM even when data doesn't.) → your binary-search LeetCode (`lc10–lc12`).
- **Bloom filter:** see §6.
- **Footer:** fixed-size trailer with offsets to the index/filter so a reader knows where
  everything is.

Because SSTables are sorted and immutable, **merging two of them is a simple linear merge**
(like merging two sorted lists — your `lc22`). Merging many is a k-way heap merge
(`lc26`). That's not a coincidence — those problems ARE this engine.

---

## 5. Flush and Compaction (the "tidy up")

**Flush:** Memtable (RAM) → new SSTable at **Level 0 (L0)**. Fast, just dumps sorted data.

**Compaction:** background process that merges SSTables into fewer, larger, cleaner files.
Why it's essential:
1. Reads get slow if a key could be in 20 files → merge to reduce file count.
2. Old/overwritten values and tombstones pile up → compaction **drops** them, reclaiming space.

**Leveled compaction (RocksDB's default):**
- **L0:** files come straight from flushes. They can have **overlapping** key ranges (file A
  and file B might both contain key "cat"). So a read may check *all* L0 files.
- **L1 and below:** files are **non-overlapping** within a level and each level is ~10× bigger
  than the one above. Non-overlapping means: for a given key, at most **one** file per level
  can contain it → binary-search the file list, check one file. Fast reads.
- Compaction picks a file in Ln, finds the overlapping files in Ln+1, merges them, writes new
  Ln+1 files, deletes the inputs.

> **The talk's twist (your Serverless-ClickHouse flagship):** there, compaction's goal isn't
> "fewer files" — it's "minimize *intersecting* files per timestamp," so a time-range query
> touches as few files as possible. Same machinery, different objective. Mini RocksDB teaches
> the machinery; the log engine reuses it with a new goal.

**Amplification — the three costs you'll benchmark:**
- **Write amplification:** 1 logical write may be physically rewritten many times as it moves
  down levels. (This is the exact cost the talk complains about with ClickHouse materialized
  views.)
- **Read amplification:** one `get` may touch several files/levels.
- **Space amplification:** dead data lingers until compaction reclaims it.

You can't win all three. Compaction strategy = choosing which to sacrifice. Measuring this
trade-off IS your Medium article.

---

## 6. Bloom filter — skip files you don't need to read

Read amplification's killer feature. A **Bloom filter** is a small bit-array that answers
"is key X in this SSTable?" with:
- **"definitely not"** — skip the file entirely (no disk read), or
- **"maybe"** — go check the file.

It never gives false negatives (never says "no" when the answer is yes), only occasional
false positives ("maybe" when actually no). So it safely skips most files that lack the key.

How it works: hash the key with k different hashes, set/test those k bits. If any tested bit
is 0 → definitely absent. Tiny (bits per key), huge read speedup on point lookups —
especially "needle in a haystack" queries (the UUID-lookup case from the talk).

**Rust you'll use:** bit manipulation, hashing (`ahash`/`xxhash`). Ties to LeetCode `lc02`
(membership intuition).

---

## 7. Tombstones — how deletes work

You can't erase a key from an immutable file. So `delete(key)` **writes a marker** — a
**tombstone** — into the Memtable/SSTable that says "this key is dead as of now."

On read, if the newest version of a key is a tombstone → report "not found," even if older
SSTables still hold a value. During compaction, once the tombstone has out-lived every older
value for that key, both are dropped and the space is reclaimed.

This is why `get` returns `Option`: `None` can mean "never existed" *or* "tombstoned." Your
`Entry` enum from Lesson 03 models exactly this.

---

## 8. Read path (put it all together)

```
get(key):
  1. Check Memtable.               hit? return (Value → Some, Tombstone → None)
  2. Check L0 SSTables newest→oldest. For each: Bloom says "maybe"? → binary-search index
                                       → read block → found? return.
  3. Check L1, L2, ... : binary-search the file list (non-overlapping), check the one file.
  4. Never found anywhere → None.
  Newest version always wins — that's why order matters.
```

Every optimization (Bloom filter, sparse index, level structure, non-overlap) exists to make
this walk touch **as little disk as possible**. That's the entire game of storage engines,
and the same instinct behind "bytes scanned / files touched" in your benchmark metrics.

---

## 9. Recovery (crash safety)

On startup:
1. Find existing SSTables (from a manifest/catalog of which files exist at which level).
2. Find the WAL(s) for any Memtable that hadn't flushed.
3. **Replay** the WAL to rebuild the in-memory Memtable.
Now state == exactly what it was before the crash. Durability achieved without ever doing a
random in-place write.

Real RocksDB tracks the file set in a **MANIFEST** (itself a log of "added file X to level N,
removed file Y"). You'll build a simplified version.

---

## 10. What we build vs skip (scope for Mini RocksDB)

**Build (v1):**
- Bytes keys/values (`Vec<u8>` / `&[u8]`)
- WAL with append + replay
- Memtable (`BTreeMap`) with `Entry = Value | Tombstone`
- SSTable writer + reader (data block, sparse index, footer)
- Bloom filter per SSTable
- Flush (Memtable → L0)
- Simple compaction (start: merge all L0 into one file; then: leveled)
- `get` across Memtable + all SSTables, newest-wins
- Recovery via WAL replay
- **Benchmark harness** (criterion): random/sequential writes, point reads, vs RocksDB.
  Measure p50/p99, throughput, write/read/space amplification, bytes written.

**Skip (v1) — add only if a later project needs it:**
- Concurrency/MVCC snapshots (add when it teaches something)
- Column families, transactions, block cache tuning
- Compression (add as a benchmark variable later)
- Distributed anything

**The $10 principle here:** the point isn't "I replaced RocksDB." It's "I reproduced the LSM
core, measured the amplification trade-offs, and found exactly where RocksDB's extra
machinery (leveled compaction, block cache, MANIFEST) becomes necessary."

---

## 11. Build order (each step = a Week of the Rust workbook)

```
1. Memtable (in-RAM, BTreeMap, Entry enum)          ← Lesson 03 already did the core
2. WAL append + replay  → put/get/delete durable
3. SSTable write (flush Memtable → sorted file) + read (scan)
4. Sparse index + footer → fast lookup in an SSTable
5. Bloom filter → skip SSTables on miss
6. get() across Memtable + L0 SSTables (newest wins)
7. Compaction (merge L0 files, drop overwrites/tombstones)
8. Recovery (replay WAL on startup)
9. Benchmark harness → graphs → vs RocksDB → Medium/LinkedIn
```

Each step compiles, runs, and has a test. By step 6 you have a working durable KV store.
Steps 7–9 are what make it *good* and give you the content.

---

## 12. The content this produces

- **GitHub:** the repo — README, this CONCEPTS doc, architecture diagram, benchmark graphs,
  "where mini-rocksdb stops and real RocksDB starts" section.
- **Medium:** "I built an LSM-tree in Rust and benchmarked it against RocksDB" —
  Problem → LSM design → implementation → benchmark → the amplification wall → lessons.
- **LinkedIn:** build-logs per milestone — "Added Bloom filters. Point-lookup misses went
  from touching 8 files to 1. p99 dropped from X to Y. Here's why."

All measured, none generic. This is flagship-prep: the LSM/SSTable/compaction machinery here
is reused directly in Mini Iceberg and Serverless ClickHouse.
```
