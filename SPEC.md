# cairn spec v0.1

## core data model

every fact is a temporal triple stored in sqlite.

### facts table

| column        | type          | description                                |
|---------------|---------------|--------------------------------------------|
| id            | TEXT (uuid)   | unique fact identifier                      |
| subject       | TEXT          | entity the fact is about (e.g. "tamish")    |
| predicate     | TEXT          | relationship (e.g. "uses_os")               |
| object        | TEXT          | value (e.g. "linux")                        |
| valid_from    | TEXT (iso8601)| when the fact became true in the world      |
| valid_until   | TEXT (iso8601)| when the fact stopped being true (NULL=open)|
| recorded_at   | TEXT (iso8601)| when the system learned the fact            |
| confidence    | REAL          | 0.0-1.0 (user=1.0, inferred=0.6, behavior=0.3) |
| source        | TEXT          | "user", "agent", "inferred", "sync"         |
| tombstone     | INTEGER       | 0=active, 1=forgotten (with reason in tombstone_reason) |
| tombstone_reason | TEXT        | why this fact was forgotten                 |
| access_count  | INTEGER       | how many times this fact was returned in recall |
| last_accessed | TEXT (iso8601)| last time this fact was returned             |
| device_id     | TEXT          | which device created this fact              |
| vector_clock  | TEXT (json)   | {"device_a": 3, "device_b": 5} for sync     |

### indexes

```sql
CREATE INDEX idx_facts_subject ON facts(subject);
CREATE INDEX idx_facts_predicate ON facts(predicate);
CREATE INDEX idx_facts_object ON facts(object);
CREATE INDEX idx_facts_active ON facts(tombstone) WHERE tombstone = 0;
CREATE INDEX idx_facts_valid ON facts(valid_from, valid_until);
CREATE INDEX idx_facts_recorded ON facts(recorded_at);
```

## operations

### remember(subject, predicate, object, options?)

inserts a new fact. if an active fact with the same (subject, predicate) exists:
- if object matches: update confidence (max), merge vector clocks, bump access_count
- if object differs: close the existing fact (set valid_until = new fact's valid_from), insert new fact

options:
- `valid_from`: defaults to now
- `confidence`: defaults to 1.0 (user-stated)
- `source`: defaults to "user"
- `device_id`: defaults to local device id

### cairn(query, options?)

returns a ranked subgraph of relevant facts.

options:
- `depth`: graph traversal depth (default 2, max 5)
- `as_of`: point-in-time query (only facts known before this time)
- `limit`: max facts returned (default 50)
- `min_confidence`: filter (default 0.0)

process:
1. semantic similarity: embed query, find top-K facts by cosine similarity on subject+object text
2. graph traversal: recursive CTE from entry points, N hops, respecting temporal validity
3. rank: confidence * recency * relevance
4. return subgraph as JSON

### forget(options)

tombstones facts that meet decay criteria. never tombstones confidence >= 0.8 unless `force=true`.

options:
- `older_than`: tombstone facts not accessed in N days
- `min_confidence`: only forget below this threshold (default 0.8)
- `dry_run`: return what would be forgotten without doing it
- `force`: bypass confidence protection

decay score:
```
decay = confidence * (1 / (1 + days_since_accessed)) * log(access_count + 1)
```

fact is forgettable if: decay < 0.1 AND confidence < 0.8

### export(format?)

exports all active (non-tombstoned) facts as a portable file. format: JSON (default) or binary.

### import(data)

imports facts from an export file. resolves conflicts using vector clocks.

### sync(peer_endpoint)

exchanges facts with a peer device since last sync point.

sync protocol:
1. exchange vector_clock deltas
2. for each fact the peer has that we don't: insert
3. for each fact both have: merge (highest confidence wins, union vector clocks)
4. for conflicting facts (same subject+predicate, different object): resolve by confidence, then by recorded_at, then by device_id lexicographic
5. update sync_log table with new sync point

## extraction

### agent-driven (MCP tool)

the agent calls `remember()` during its response. zero extra LLM calls. the agent decides what's worth remembering.

### pattern-based (post-turn hook)

runs after each conversation turn. no LLM. regex + simple NLP patterns:

| pattern                              | extracts                          |
|--------------------------------------|-----------------------------------|
| "my name is X" / "i'm X" / "i am X"  | remember(user, name, X)           |
| "i use X" / "i'm using X"            | remember(user, uses, X)           |
| "i work at X" / "i'm at X"          | remember(user, works_at, X)        |
| "i prefer X" / "i like X"           | remember(user, prefers, X, confidence=0.6) |
| "i live in X" / "i'm in X"          | remember(user, lives_in, X)        |
| "X is Y" (about known entities)      | remember(X, is, Y)                 |

deduplication: before inserting, check for active fact with same (subject, predicate, object). if exists, bump access_count instead of duplicating.

## MCP tools

recall exposes these as MCP tools. any MCP-compatible agent gets memory for free.

| tool      | args                              | returns        |
|-----------|-----------------------------------|----------------|
| remember  | subject, predicate, object, confidence? | fact id  |
| recall    | query, depth?, as_of?, limit?     | subgraph JSON  |
| forget    | older_than?, min_confidence?, dry_run? | count   |
| export    |                                   | JSON string    |
| import    | data                              | count          |

## file format

one sqlite database file. default location: `~/.cairn/memory.db`

no config files. no external services. no network calls. the file IS the product.

## what this is not

- not a vector database. semantic similarity is brute-force cosine on a small index (memory is small).
- not a graph database. traversal is recursive CTE in sqlite. no neo4j, no cypher.
- not a cloud service. no server required. sync is peer-to-peer.
- not a protocol spec with no implementation. the reference implementation IS the spec.
