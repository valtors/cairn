use chrono::{DateTime, Utc};
use recall_store::{Fact, Store};

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
    let max_confidence = if opts.force { 2.0 } else { opts.min_confidence.unwrap_or(0.8) };
    let days = opts.older_than_days.unwrap_or(30);

    let candidates = store.get_stale_facts(days, max_confidence)?;
    let mut forgotten = Vec::new();
    let mut kept = 0;
    let mut reasons = Vec::new();

    for fact in candidates {
        let score = decay_score(&fact);
        if score < 0.1 && (fact.confidence < max_confidence || opts.force) {
            reasons.push((fact.id.clone(), format!("decay={:.3} conf={:.2}", score, fact.confidence)));
            if !opts.dry_run {
                store.tombstone(&fact.id, &format!("decay={:.3}", score))?;
            }
            forgotten.push(fact.id);
        } else {
            kept += 1;
        }
    }

    Ok(ForgetResult { forgotten, kept, reasons })
}
