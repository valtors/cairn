//! MCP server exposing remember / cairn / forget / export / import / extract.
//!
//! any MCP-compatible agent gets memory for free. the agent calls
//! `remember()` during its response. the agent calls `cairn()` to query.
//! that's it. two commands that turn any agent into one that remembers.
//!
//! protocol: JSON-RPC 2.0 over stdio. no HTTP, no SSE, no WebSocket.
//! the agent talks, cairn listens, cairn answers.

use cairn_extract::extract_from_text;
use cairn_forget::{run as run_forget, ForgetOptions};
use cairn_query::{query, QueryOptions};
use cairn_store::{RememberOptions, Store};
use serde_json::{json, Value};

pub fn handle_remember(store: &Store, args: &Value) -> Result<Value, String> {
    let subject = args.get("subject").and_then(|v| v.as_str()).ok_or("missing subject")?;
    let predicate = args.get("predicate").and_then(|v| v.as_str()).ok_or("missing predicate")?;
    let object = args.get("object").and_then(|v| v.as_str()).ok_or("missing object")?;
    let confidence = args.get("confidence").and_then(|v| v.as_f64());
    let source = args.get("source").and_then(|v| v.as_str());

    let opts = RememberOptions {
        valid_from: None,
        recorded_at: None,
        confidence,
        source: source.map(|s| s.to_string()),
        device_id: None,
    };

    let id = store.remember(subject, predicate, object, opts)?;
    Ok(json!({ "id": id, "status": "remembered" }))
}

pub fn handle_recall(store: &Store, args: &Value) -> Result<Value, String> {
    let query_text = args.get("query").and_then(|v| v.as_str()).ok_or("missing query")?;
    let depth = args.get("depth").and_then(|v| v.as_u64()).map(|d| d as usize).unwrap_or(2);
    let limit = args.get("limit").and_then(|v| v.as_u64()).map(|l| l as usize).unwrap_or(50);
    let as_of = args.get("as_of").and_then(|v| v.as_str()).map(|s| s.to_string());
    let min_confidence = args.get("min_confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let opts = QueryOptions {
        depth,
        as_of,
        limit,
        min_confidence,
    };

    let result = query(store, query_text, opts)?;
    Ok(json!({
        "facts": result.facts,
        "entry_points": result.entry_points,
    }))
}

pub fn handle_forget(store: &Store, args: &Value) -> Result<Value, String> {
    let older_than = args.get("older_than_days").and_then(|v| v.as_i64());
    let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    let opts = ForgetOptions {
        older_than_days: older_than,
        min_confidence: Some(0.8),
        dry_run,
        force,
    };

    let result = run_forget(store, opts)?;
    Ok(json!({
        "forgotten": result.forgotten,
        "kept": result.kept,
        "dry_run": dry_run,
        "reasons": result.reasons.iter().map(|(id, reason)| json!({"id": id, "reason": reason})).collect::<Vec<_>>(),
    }))
}

pub fn handle_export(store: &Store) -> Result<Value, String> {
    let facts = store.all_facts()?;
    Ok(json!({ "facts": facts, "count": facts.len() }))
}

pub fn handle_import(store: &Store, args: &Value) -> Result<Value, String> {
    let facts = args.get("facts").ok_or("missing facts array")?;
    let fact_arr = facts.as_array().ok_or("facts must be an array")?;
    let mut imported = 0;
    for fact_val in fact_arr {
        let fact: cairn_store::Fact = serde_json::from_value(fact_val.clone()).map_err(|e| e.to_string())?;
        store.import_fact(&fact)?;
        imported += 1;
    }
    Ok(json!({ "imported": imported }))
}

pub fn handle_extract(store: &Store, args: &Value) -> Result<Value, String> {
    let text = args.get("text").and_then(|v| v.as_str()).ok_or("missing text")?;
    let user_name = args.get("user_name").and_then(|v| v.as_str());

    let extracted = extract_from_text(text, user_name);
    let mut stored = 0;
    for fact in &extracted {
        let opts = RememberOptions {
            valid_from: None,
            recorded_at: None,
            confidence: Some(fact.confidence),
            source: Some(fact.source.clone()),
            device_id: None,
        };
        store.remember(&fact.subject, &fact.predicate, &fact.object, opts)?;
        stored += 1;
    }
    Ok(json!({
        "extracted": extracted.len(),
        "stored": stored,
    }))
}

pub fn list_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "remember",
            "description": "Store a fact. The agent calls this to remember something about the user or a topic.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "subject": { "type": "string", "description": "Entity the fact is about (e.g. 'tamish')" },
                    "predicate": { "type": "string", "description": "Relationship (e.g. 'uses_os')" },
                    "object": { "type": "string", "description": "Value (e.g. 'linux')" },
                    "confidence": { "type": "number", "description": "0.0-1.0, default 1.0" },
                    "source": { "type": "string", "description": "user, agent, inferred" }
                },
                "required": ["subject", "predicate", "object"]
            }
        }),
        json!({
            "name": "cairn",
            "description": "Query memory. Returns relevant facts by semantic similarity and graph traversal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language query" },
                    "depth": { "type": "integer", "description": "Graph traversal depth (default 2, max 5)" },
                    "limit": { "type": "integer", "description": "Max facts returned (default 50)" },
                    "as_of": { "type": "string", "description": "Point-in-time query (ISO8601)" },
                    "min_confidence": { "type": "number", "description": "Filter (default 0.0)" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "forget",
            "description": "Run garbage collection on stale memories. Tombstones facts that have decayed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "older_than_days": { "type": "integer", "description": "Tombstone facts not accessed in N days (default 30)" },
                    "dry_run": { "type": "boolean", "description": "Preview without deleting (default false)" },
                    "force": { "type": "boolean", "description": "Bypass confidence protection (default false)" }
                }
            }
        }),
        json!({
            "name": "export_memory",
            "description": "Export all facts as JSON. For backup or transfer to another device.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "import_memory",
            "description": "Import facts from a JSON export.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "facts": { "type": "array", "description": "Array of fact objects from an export" }
                },
                "required": ["facts"]
            }
        }),
        json!({
            "name": "extract",
            "description": "Extract facts from conversation text using pattern matching. No LLM calls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Conversation text to extract from" },
                    "user_name": { "type": "string", "description": "Optional name to use as subject" }
                },
                "required": ["text"]
            }
        }),
    ]
}

pub fn dispatch(store: &Store, tool: &str, args: &Value) -> Result<Value, String> {
    match tool {
        "remember" => handle_remember(store, args),
        "cairn" => handle_recall(store, args),
        "forget" => handle_forget(store, args),
        "export_memory" => handle_export(store),
        "import_memory" => handle_import(store, args),
        "extract" => handle_extract(store, args),
        _ => Err(format!("unknown tool: {}", tool)),
    }
}
