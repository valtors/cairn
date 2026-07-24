//! forgetting as a first-class operation.
//!
//! facts decay by confidence, access frequency, and age. stale facts get
//! tombstoned (soft delete with reason). forgetting is auditable. memory
//! gets better over time, not just bigger.
//!
//! facts with confidence >= 0.8 are never forgotten unless `force` is set.
//! the agent's direct knowledge is protected. inferred patterns are not.

use cairn_store::{Fact, Store};
use chrono::{DateTime, Utc};

pub struct ForgetOptions {
    pub older_than_days: Option<i64>,
    pub min_confidence: Option<f64>,
    pub dry_run: bool,
    pub force: bool,
}

impl Default for ForgetOptions {
    fn default() -> Self {
        Self {
            older_than_days: Some(30),
            min_confidence: Some(0.8),
            dry_run: false,
            force: false,
        }
    }
}

pub struct ForgetResult {
    pub forgotten: Vec<String>,
    pub kept: usize,
    pub reasons: Vec<(String, String)>,
}

/// decay = confidence * (1 / (1 + days_since_accessed)) * log(access_count + 1)
///
/// a fact that was never accessed and has low confidence decays fast.
/// a fact that was accessed recently and has high confidence stays.
/// a fact with confidence >= 0.8 is immune unless force is set.
pub fn decay_score(fact: &Fact) -> f64 {
    let confidence = fact.confidence;
    let now = Utc::now();
    let last = fact
        .last_accessed
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(now);
    let days_since = (now - last).num_days().max(0) as f64;
    let access_factor = (fact.access_count as f64 + 1.0).ln();
    confidence * (1.0 / (1.0 + days_since)) * access_factor
}

pub fn run(store: &Store, opts: ForgetOptions) -> Result<ForgetResult, String> {
    let max_confidence = if opts.force {
        2.0
    } else {
        opts.min_confidence.unwrap_or(0.8)
    };
    let days = opts.older_than_days.unwrap_or(30);

    let candidates = store.get_stale_facts(days, max_confidence)?;
    let mut forgotten = Vec::new();
    let mut kept = 0;
    let mut reasons = Vec::new();

    for fact in candidates {
        let score = decay_score(&fact);
        if score < 0.1 && (fact.confidence < max_confidence || opts.force) {
            reasons.push((
                fact.id.clone(),
                format!("decay={:.3} conf={:.2}", score, fact.confidence),
            ));
            if !opts.dry_run {
                store.tombstone(&fact.id, &format!("decay={:.3}", score))?;
            }
            forgotten.push(fact.id);
        } else {
            kept += 1;
        }
    }

    Ok(ForgetResult {
        forgotten,
        kept,
        reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cairn_store::{RememberOptions, Store};

    fn test_store() -> Store {
        let path = format!(
            "/home/container/cairn-f-{}-{}.db",
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
    fn forget_low_confidence() {
        let store = test_store();
        store
            .remember(
                "s",
                "p",
                "o",
                RememberOptions {
                    confidence: Some(0.2),
                    source: Some("inferred".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        let result = run(
            &store,
            ForgetOptions {
                min_confidence: Some(0.8),
                older_than_days: Some(0),
                dry_run: false,
                force: false,
            },
        )
        .unwrap();
        assert!(!result.forgotten.is_empty());
    }

    #[test]
    fn forget_dry_run() {
        let store = test_store();
        store
            .remember(
                "s",
                "p",
                "o",
                RememberOptions {
                    confidence: Some(0.2),
                    source: Some("inferred".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        let result = run(
            &store,
            ForgetOptions {
                min_confidence: Some(0.8),
                older_than_days: Some(0),
                dry_run: true,
                force: false,
            },
        )
        .unwrap();
        assert!(!result.forgotten.is_empty());
        let active = store.get_active_facts().unwrap();
        assert!(!active.is_empty());
    }

    #[test]
    fn forget_protects_high_confidence() {
        let store = test_store();
        store
            .remember(
                "s",
                "p",
                "o",
                RememberOptions {
                    confidence: Some(1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        let result = run(
            &store,
            ForgetOptions {
                min_confidence: Some(0.8),
                older_than_days: Some(0),
                dry_run: false,
                force: false,
            },
        )
        .unwrap();
        assert!(result.forgotten.is_empty());
    }
}
