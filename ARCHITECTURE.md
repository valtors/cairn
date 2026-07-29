# Architecture

## Overview

Cairn is a temporal knowledge store for AI agents. It persists facts as time-stamped triples in a single SQLite file. Facts have validity windows, confidence scores, and access tracking. When a new fact contradicts an old one, the old fact closes -- it does not delete. You can query the past.

```
  +----------+     +----------+
  |  CLI     |     |  MCP     |
  |  (clap)  |     |  (stdio) |
  +----+-----+     +----+-----+
       |                |
       +-------+--------+
               |
          +----v----+
          |  store   |
          |  (sqlite)|
          +----+----+
               |
     +---------+---------+---------+---------+
     |         |         |         |         |
     v         v         v         v         v
  +--+--+  +--+--+  +--+--+  +--+--+  +--+--+
  |traverse| |query| |forget| |extract| |sync |
  |(graph)| |(rank)| |(decay)| (regex)| (vec)|
  +-------+ +-----+ +-------+ +-------+ +-----+
```

## Design Principles

1. **Triples, not documents.** A fact is (subject, predicate, object) with metadata. Not a blob of text. The agent can reason about relationships, not just recall paragraphs.
2. **Time is first-class.** Every fact has `valid_from` and `valid_until`. Querying "what did the agent know on July 1?" is a first-class operation. Facts that contradict close, they do not delete.
3. **Forgetting is a feature.** Facts decay by confidence, access frequency, and age. Stale facts get tombstoned with a reason. Memory gets better over time, not just bigger. Facts with confidence >= 0.8 are immune unless force is set.
4. **Zero LLM for extraction.** Pattern-based fact extraction catches 80% of facts (name, tools, location, preferences) with regex. No API calls. The other 20% come from the agent calling `remember()` explicitly.
5. **Federated sync.** Devices exchange bundles via vector clocks. Conflict resolution is deterministic: highest confidence wins, then most recent, then device ID. No server required.
6. **One SQLite file.** No external services, no embeddings, no vector database. Memory is small. Text matching is fast enough.

## Components

### store (`crates/store`)

The persistence layer. SQLite with WAL mode, foreign keys, and a single `facts` table.

**Schema:**
```sql
CREATE TABLE facts (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    object TEXT NOT NULL,
    valid_from TEXT NOT NULL,
    valid_until TEXT,
    recorded_at TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'user',
    tombstone INTEGER NOT NULL DEFAULT 0,
    tombstone_reason TEXT,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed TEXT,
    device_id TEXT NOT NULL,
    vector_clock TEXT NOT NULL DEFAULT '{}'
);
```

**Fact lifecycle:**
1. `remember(subject, predicate, object)` -- inserts a new fact with UUID, current timestamp, and default confidence 1.0.
2. If a fact with the same (subject, predicate) exists and is active, its `valid_until` is set to now. The old fact closes; the new fact opens.
3. `touch(id)` -- increments `access_count` and updates `last_accessed`. Used during queries to track fact usage.
4. `tombstone(id, reason)` -- soft delete. Sets `tombstone=1` and records why. The fact is excluded from queries but remains in the database for audit.

**Indexes:** subject, predicate, object, active (partial index on `tombstone=0`), valid_from/valid_until, recorded_at.

### traverse (`crates/traverse`)

Graph traversal over temporal facts. Given entry subjects, walks the graph by following objects to their own facts. Depth-limited BFS.

**Algorithm:**
1. Start with entry subjects (e.g., "tamish").
2. For each subject, find all active facts where `subject = X`.
3. Each fact's `object` becomes a new subject to explore.
4. Track visited subjects to prevent cycles.
5. Respect temporal validity: if `as_of` is set, only return facts that were recorded before and still valid at that timestamp.
6. Return all facts and the hops taken.

No Cypher, no Neo4j. Application-level recursive CTE in Rust.

### query (`crates/query`)

Natural language entry point. The agent says `recall("what os does tamish use")` and gets back a subgraph.

**Pipeline:**
1. **Entry point detection:** brute-force text matching on subject + object across all active facts. Not a vector database. Memory is small; text matching is fast enough.
2. **Graph traversal:** pass entry subjects to `traverse()` with configured depth (default 2).
3. **Ranking:** each fact gets a relevance score based on text match quality, confidence, and recency.
4. **Filtering:** apply `min_confidence` and `limit` constraints.
5. **Touch:** increment access count on returned facts.

**Output:** `QueryResult` with facts (ranked), entry points used.

### forget (`crates/forget`)

Forgetting as a first-class operation. Facts decay over time.

**Decay score:** `confidence * (1 / (1 + days_since_accessed)) * ln(access_count + 1)`

A fact that was never accessed and has low confidence decays fast. A fact that was accessed recently and has high confidence stays. Facts with confidence >= 0.8 are immune unless `force` is set.

**Forget options:**
- `older_than_days` -- only consider facts older than N days (default 30).
- `min_confidence` -- facts above this confidence are immune (default 0.8).
- `dry_run` -- report what would be forgotten without tombstoning.
- `force` -- override the confidence immunity.

**Result:** list of forgotten fact IDs, kept count, and reasons.

### extract (`crates/extract`)

Pattern-based fact extraction. Zero LLM calls.

**Patterns:**
| Regex | Predicate | Confidence |
|---|---|---|
| "my name is X" / "i'm X" / "i am X" | name | 1.0 |
| "i use X" / "i'm using X" | uses | 1.0 |
| "i work at X" / "i'm at X" | works_at | 1.0 |
| "i prefer X" / "i like X" / "i love X" | prefers | 0.6 |
| "i live in X" / "i'm in X" | lives_in | 1.0 |
| "i hate X" / "i can't stand X" | dislikes | 0.7 |
| "my favorite X is Y" | favorite | 0.8 |

Stop words are trimmed from extracted objects. Objects longer than 50 chars are rejected (avoids capturing full sentences).

### sync (`crates/sync`)

Federated sync between devices. Peer-to-peer via vector clocks.

**Export:** `export_bundle(store)` serializes all facts with the local device ID.

**Import:** `import_bundle(store, bundle)` processes incoming facts:
1. For each fact, check if a conflicting fact exists (same subject + predicate).
2. **Conflict resolution:**
   - Highest confidence wins.
   - Tie: most recent `recorded_at` wins.
   - Tie: highest device ID wins (deterministic, no ambiguity).
3. If incoming wins: insert the new fact, close the existing one.
4. If existing wins: touch the existing fact (update access count), skip incoming.
5. If no conflict: insert directly.

No server. Devices exchange JSON bundles directly (file, AirDrop, sync server -- the transport is not cairn's problem).

### mcp (`crates/mcp`)

MCP server over stdio. Exposes cairn as an MCP tool to any MCP-compatible agent.

**Protocol:** JSON-RPC 2.0 over stdio. No HTTP, no SSE, no WebSocket.

**Tools:**
- `remember(subject, predicate, object, confidence?, source?)` -- store a fact.
- `recall(query, depth?, limit?, as_of?, min_confidence?)` -- query the memory.
- `forget(older_than_days?, dry_run?, force?)` -- run decay.
- `export()` -- export all facts as JSON.
- `import(file)` -- import facts from JSON.
- `extract(text, user_name?)` -- extract facts from text.

### CLI (`bin/cairn`)

Command-line interface using clap.

| Command | Purpose |
|---|---|
| `remember --subject X --predicate Y --object Z` | Store a fact |
| `recall "query text" --depth 2 --limit 50` | Query memory |
| `forget --older-than-days 30 --dry-run` | Run decay |
| `export` | Export to stdout as JSON |
| `import --file memory.json` | Import from file |
| `extract "text" --user-name tamish` | Extract facts from text |
| `serve` | Run as MCP server on stdio |

DB path: `$CAIRN_DB` env var or `~/.cairn/memory.db` (default).

## Testing

63 tests across all crates. Integration tests in `tests/integration.rs` exercise the full pipeline: remember, recall, forget, export, import, extract, sync.

## Dependencies

- `rusqlite` -- SQLite (bundled, no system SQLite needed)
- `serde` / `serde_json` -- serialization
- `chrono` -- timestamps
- `uuid` -- fact IDs
- `clap` -- CLI parsing
- `regex` -- pattern extraction
- `hostname` -- device identification

## Published Crates

All published to crates.io:
- `cairn-memory` (binary: `cairn`) -- main CLI
- `cairn-store` -- storage engine
- `cairn-traverse` -- graph traversal
- `cairn-query` -- query + ranking
- `cairn-forget` -- decay engine
- `cairn-extract-lib` -- fact extraction
- `cairn-sync` -- federated sync
- `cairn-mcp` -- MCP server

Install: `cargo install cairn-memory`
