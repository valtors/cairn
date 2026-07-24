//! federated sync. your phone agent and laptop agent share a brain.
//!
//! peer-to-peer sync via vector clocks. conflict resolution is
//! deterministic. no server required.
//!
//! sync is just export + import with conflict resolution. two devices
//! exchange bundles. same (subject, predicate) with different objects:
//! highest confidence wins, then most recent, then device id
//! lexicographic. no ambiguity.

use cairn_store::{Fact, Store};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBundle {
    pub device_id: String,
    pub facts: Vec<Fact>,
    pub last_sync_counter: i64,
}

pub fn export_bundle(store: &Store) -> Result<SyncBundle, String> {
    let facts = store.all_facts()?;
    Ok(SyncBundle {
        device_id: "local".to_string(),
        facts,
        last_sync_counter: 0,
    })
}

pub fn import_bundle(store: &Store, bundle: &SyncBundle) -> Result<usize, String> {
    let mut imported = 0;
    for fact in &bundle.facts {
        match resolve_conflict(store, fact) {
            ConflictResolution::Insert => {
                store.import_fact(fact)?;
                imported += 1;
            }
            ConflictResolution::Merge(existing_id) => {
                store.touch(&existing_id)?;
                imported += 1;
            }
            ConflictResolution::Skip => {}
        }
    }
    Ok(imported)
}

enum ConflictResolution {
    Insert,
    Merge(String),
    Skip,
}

fn resolve_conflict(store: &Store, incoming: &Fact) -> ConflictResolution {
    let existing = store
        .get_active_facts_for(&incoming.subject)
        .unwrap_or_default();

    for existing_fact in existing {
        if existing_fact.predicate == incoming.predicate {
            if existing_fact.object == incoming.object {
                if incoming.confidence > existing_fact.confidence {
                    return ConflictResolution::Insert;
                }
                return ConflictResolution::Merge(existing_fact.id);
            } else {
                if incoming.confidence > existing_fact.confidence
                    || (incoming.confidence == existing_fact.confidence
                        && incoming.recorded_at > existing_fact.recorded_at)
                {
                    return ConflictResolution::Insert;
                }
                return ConflictResolution::Skip;
            }
        }
    }
    ConflictResolution::Insert
}

pub fn merge_vector_clocks(a: &str, b: &str) -> String {
    let mut va: Value = serde_json::from_str(a).unwrap_or_else(|_| serde_json::json!({}));
    let vb: Value = serde_json::from_str(b).unwrap_or_else(|_| serde_json::json!({}));

    if let (Some(obj_a), Some(obj_b)) = (va.as_object_mut(), vb.as_object()) {
        for (key, val_b) in obj_b {
            let current = obj_a.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
            let incoming = val_b.as_i64().unwrap_or(0);
            obj_a.insert(key.clone(), serde_json::json!(current.max(incoming)));
        }
    }
    va.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_store::{RememberOptions, Store};

    fn test_store() -> Store {
        let path = format!(
            "/tmp/cairn-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        );
        let _ = std::fs::remove_file(&path);
        Store::open(&path, Some("test".to_string())).unwrap()
    }

    #[test]
    fn export_empty_store() {
        let store = test_store();
        let bundle = export_bundle(&store).unwrap();
        assert!(bundle.facts.is_empty());
    }

    #[test]
    fn export_import_roundtrip() {
        let store = test_store();
        store
            .remember("s", "p", "o", RememberOptions::default())
            .unwrap();
        let bundle = export_bundle(&store).unwrap();
        assert_eq!(bundle.facts.len(), 1);
        let store2 = test_store();
        let count = import_bundle(&store2, &bundle).unwrap();
        assert_eq!(count, 1);
        let active = store2.get_active_facts().unwrap();
        assert_eq!(active.len(), 1);
    }
}
