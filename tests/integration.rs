use cairn_store::{RememberOptions, Store};
use cairn_query::{query, QueryOptions};
use cairn_forget::{run as run_forget, ForgetOptions};
use cairn_extract::extract_from_text;
use cairn_sync::{export_bundle, import_bundle};
use serde_json::json;

fn tmp_store() -> Store {
    let path = format!("/home/container/cairn-test-{}.db", std::process::id());
    let _ = std::fs::remove_file(&path);
    Store::open(&path, Some("test-device".to_string())).unwrap()
}

#[test]
fn remember_and_recall() {
    let store = tmp_store();

    let id = store.remember("tamish", "uses_os", "linux", RememberOptions::default()).unwrap();
    assert!(!id.is_empty());

    let result = query(&store, "what os does tamish use", QueryOptions::default()).unwrap();
    assert!(!result.facts.is_empty());
    assert_eq!(result.facts[0].subject, "tamish");
    assert_eq!(result.facts[0].object, "linux");
}

#[test]
fn contradiction_closes_old_fact_not_deletes() {
    let store = tmp_store();

    store.remember("tamish", "uses_os", "macos", RememberOptions::default()).unwrap();
    store.remember("tamish", "uses_os", "linux", RememberOptions::default()).unwrap();

    let active = store.get_active_facts_for("tamish").unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].object, "linux");

    let all = store.all_facts().unwrap();
    assert_eq!(all.len(), 2);
    let macos_fact = all.iter().find(|f| f.object == "macos").unwrap();
    assert!(macos_fact.valid_until.is_some());
}

#[test]
fn point_in_time_query() {
    let store = tmp_store();

    let old_time = "2024-01-01T00:00:00Z".to_string();
    store.remember("tamish", "uses_os", "macos", RememberOptions {
        valid_from: Some(old_time.clone()),
        recorded_at: Some(old_time.clone()),
        ..Default::default()
    }).unwrap();

    let new_time = "2025-07-15T00:00:00Z".to_string();
    store.remember("tamish", "uses_os", "linux", RememberOptions {
        valid_from: Some(new_time.clone()),
        recorded_at: Some(new_time.clone()),
        ..Default::default()
    }).unwrap();

    let facts_2024 = store.facts_as_of("2024-06-01T00:00:00Z").unwrap();
    let os_facts: Vec<_> = facts_2024.iter().filter(|f| f.predicate == "uses_os").collect();
    assert_eq!(os_facts.len(), 1);
    assert_eq!(os_facts[0].object, "macos");

    let facts_2025 = store.facts_as_of("2025-08-01T00:00:00Z").unwrap();
    let os_facts: Vec<_> = facts_2025.iter().filter(|f| f.predicate == "uses_os").collect();
    assert_eq!(os_facts.len(), 1);
    assert_eq!(os_facts[0].object, "linux");
}

#[test]
fn same_fact_updates_confidence_not_duplicates() {
    let store = tmp_store();

    store.remember("tamish", "name", "tamish", RememberOptions {
        confidence: Some(0.5),
        ..Default::default()
    }).unwrap();
    store.remember("tamish", "name", "tamish", RememberOptions {
        confidence: Some(1.0),
        ..Default::default()
    }).unwrap();

    let active = store.get_active_facts_for("tamish").unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].confidence, 1.0);
    assert!(active[0].access_count > 0);
}

#[test]
fn forget_tombstones_low_confidence() {
    let store = tmp_store();

    store.remember("user", "prefers", "dark mode", RememberOptions {
        confidence: Some(0.3),
        source: Some("inferred".to_string()),
        ..Default::default()
    }).unwrap();

    let result = run_forget(&store, ForgetOptions {
        older_than_days: Some(0),
        min_confidence: Some(0.8),
        dry_run: false,
        force: false,
    }).unwrap();

    assert!(!result.forgotten.is_empty());

    let active = store.get_active_facts().unwrap();
    let found = active.iter().any(|f| f.object == "dark mode");
    assert!(!found);
}

#[test]
fn forget_protects_high_confidence() {
    let store = tmp_store();

    store.remember("tamish", "name", "tamish", RememberOptions {
        confidence: Some(1.0),
        ..Default::default()
    }).unwrap();

    let result = run_forget(&store, ForgetOptions {
        older_than_days: Some(0),
        min_confidence: Some(0.8),
        dry_run: false,
        force: false,
    }).unwrap();

    assert!(result.forgotten.is_empty());

    let active = store.get_active_facts().unwrap();
    assert!(!active.is_empty());
}

#[test]
fn extract_patterns_from_text() {
    let facts = extract_from_text("my name is damir and i use linux", Some("user"));
    assert!(facts.iter().any(|f| f.predicate == "name" && f.object == "damir"));
    assert!(facts.iter().any(|f| f.predicate == "uses" && f.object == "linux"));
}

#[test]
fn extract_with_context() {
    let facts = extract_from_text("i work at juice dev and i live in bangalore", Some("user"));
    assert!(facts.iter().any(|f| f.predicate == "works_at" && f.object == "juice dev"));
    assert!(facts.iter().any(|f| f.predicate == "lives_in" && f.object == "bangalore"));
}

#[test]
fn export_import_roundtrip() {
    let store = tmp_store();

    store.remember("tamish", "uses_os", "linux", RememberOptions::default()).unwrap();
    store.remember("tamish", "name", "tamish", RememberOptions::default()).unwrap();

    let bundle = export_bundle(&store).unwrap();
    assert_eq!(bundle.facts.len(), 2);

    let store2 = tmp_store();
    let imported = import_bundle(&store2, &bundle).unwrap();
    assert_eq!(imported, 2);

    let result = query(&store2, "what os does tamish use", QueryOptions::default()).unwrap();
    assert!(!result.facts.is_empty());
    let os_fact = result.facts.iter().find(|f| f.predicate == "uses_os");
    assert!(os_fact.is_some());
    assert_eq!(os_fact.unwrap().object, "linux");
}

#[test]
fn graph_traversal_follows_objects() {
    let store = tmp_store();

    store.remember("tamish", "works_at", "valtors", RememberOptions::default()).unwrap();
    store.remember("valtors", "builds", "cairn", RememberOptions::default()).unwrap();
    store.remember("cairn", "is", "memory store", RememberOptions::default()).unwrap();

    let result = query(&store, "tamish", QueryOptions {
        depth: 3,
        ..Default::default()
    }).unwrap();

    let objects: Vec<&str> = result.facts.iter().map(|f| f.object.as_str()).collect();
    assert!(objects.contains(&"valtors"));
    assert!(objects.contains(&"cairn"));
    assert!(objects.contains(&"memory store"));
}
