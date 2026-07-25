# cairn

[![CI](https://github.com/valtors/cairn/actions/workflows/ci.yml/badge.svg)](https://github.com/valtors/cairn/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-0F172A?style=flat-square)](./LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-CE422B?style=flat-square)](https://www.rust-lang.org/)
[![tests](https://img.shields.io/badge/tests-63-green?style=flat-square)]()

agent wayfinding. temporal knowledge store in one sqlite file. no neo4j, no cloud, no lock-in.

## what

cairn is a new category. not "memory" or "knowledge graph." agent wayfinding - agents that can find their way back to what they know.

a cairn is a pile of stones hikers stack to mark a path. agents leave cairns to mark what they learned and where they've been. each fact is a stone. the pile is the path.

MCP gave agents tools. cairn gives them a path.

## why

every agent memory system today is either a walled garden (mem0, zep cloud) or a heavy dependency (neo4j, postgres). none of them talk to each other. your memory is locked into whichever platform you picked first. cairn is the opposite: one binary, one file, zero dependencies, works with any agent that speaks MCP.

| | mem0 | zep | neo4j | cairn |
|---|---|---|---|---|
| setup | pip + api key | docker + api key | docker + cypher | one binary |
| dependencies | python, redis | docker, postgres | jvm, 2gb+ ram | sqlite |
| protocol | rest api | rest api | cypher/bolt | MCP |
| temporal queries | no | limited | no | yes |
| forgetting | no | no | no | yes |
| federated sync | no | no | no | yes |
| works offline | no | no | yes | yes |

## how it works

cairn stores facts as temporal triples: subject, predicate, object, with validity windows.

```
tamish --uses_os--> macos    (valid: 2024-01-01 to 2025-07-15)
tamish --uses_os--> linux     (valid: 2025-07-15 to now)
```

when a new fact contradicts an old one, the old fact is closed (not deleted). you can query the past: "what did we know about tamish in march?"

### five things cairn does that nobody else does together

1. **bi-temporal tracking without a graph database.** every fact carries two timestamps: when it was true in the world, and when the system learned it. contradicted facts get closed, not deleted. all in sqlite.

2. **extraction without burning LLM calls.** the agent calls `remember()` as an MCP tool during its normal response. no separate extraction pipeline. no extra API calls. a pattern-based post-turn hook catches 80% of facts for free.

3. **forgetting as a first-class operation.** facts decay by confidence, access frequency, and age. stale facts get tombstoned (soft delete with reason). forgetting is auditable. memory gets better over time, not just bigger.

4. **federated sync.** your phone agent and laptop agent share a brain. peer-to-peer sync via vector clocks. conflict resolution is deterministic. no server required.

5. **query by meaning, not by query language.** the agent says `recall("what do you know about tamish's setup")` and gets back a subgraph. no cypher, no SQL. semantic similarity finds entry points, graph traversal follows relationships, ranking returns what matters.

## architecture

```
cairn/
  crates/
    store/        temporal sqlite engine, validity windows, conflict resolution
    traverse/     graph traversal, depth-limited
    forget/       decay scoring, garbage collection, tombstones
    query/        semantic entry points + traversal + ranking
    extract/      pattern-based fact extraction (no LLM)
    sync/         vector clocks, peer sync, conflict resolution
    mcp/          MCP server exposing remember/recall/forget/export
  bin/
    cairn/        CLI + MCP server entry point
```

one sqlite file. one binary. zero external services.

## install

```bash
cargo install cairn
```

or build from source:

```bash
git clone https://github.com/valtors/cairn
cd cairn
cargo build --release
cp target/release/cairn /usr/local/bin/
```

## usage

```bash
# run as MCP server (any MCP-compatible agent connects)
cairn serve

# or use directly
cairn remember --subject tamish --predicate uses_os --object linux
cairn recall "what os does tamish use"
cairn forget --older-than 30d
cairn export > my-memory.json
cairn import < my-memory.json
```

### with claude desktop

```json
{
  "mcpServers": {
    "cairn": {
      "command": "cairn",
      "args": ["serve"]
    }
  }
}
```

## tests

63 tests across 6 crates. all pass.

```bash
cargo test --workspace
```

## license

MIT. strictly open source. no cloud tier, no enterprise plan, no proprietary fork.
