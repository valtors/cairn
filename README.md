# cairn

agent wayfinding. temporal knowledge store in one sqlite file. no neo4j, no cloud, no lock-in.

## what

cairn is a new category. not "memory" or "knowledge graph." agent wayfinding - agents that can find their way back to what they know.

a cairn is a pile of stones hikers stack to mark a path. agents leave cairns to mark what they learned and where they've been. each fact is a stone. the pile is the path.

MCP gave agents tools. cairn gives them a path.

## why

every agent memory system today is either a walled garden (mem0, zep cloud) or a heavy dependency (neo4j, postgres). none of them talk to each other. your memory is locked into whichever platform you picked first. cairn is the opposite: one binary, one file, zero dependencies, works with any agent that speaks MCP.

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

## usage

```bash
# install
cargo install cairn

# run as MCP server (any MCP-compatible agent connects)
cairn serve

# or use directly
cairn remember --subject tamish --predicate uses_os --object linux
cairn recall "what os does tamish use"
cairn forget --older-than 30d
cairn export > my-memory.json
cairn import < my-memory.json
```

## license

MIT. strictly open source. no cloud tier, no enterprise plan, no proprietary fork.
