use cairn_store::{Fact, Store};
use cairn_traverse::traverse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub facts: Vec<FactNode>,
    pub entry_points: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactNode {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f64,
    pub recorded_at: String,
    pub valid_from: String,
    pub valid_until: Option<String>,
    pub score: f64,
}

pub struct QueryOptions {
    pub depth: usize,
    pub as_of: Option<String>,
    pub limit: usize,
    pub min_confidence: f64,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            depth: 2,
            as_of: None,
            limit: 50,
            min_confidence: 0.0,
        }
    }
}

pub fn query(store: &Store, query_text: &str, opts: QueryOptions) -> Result<QueryResult, String> {
    let facts = store.get_active_facts()?;
    let entry_points = find_entry_points(&facts, query_text);

    if entry_points.is_empty() {
        return Ok(QueryResult {
            facts: vec![],
            entry_points: vec![],
        });
    }

    let result = traverse(
        store.conn(),
        &entry_points,
        opts.depth,
        opts.as_of.as_deref(),
    )?;

    let mut nodes: Vec<FactNode> = result
        .facts
        .into_iter()
        .filter(|f| f.confidence >= opts.min_confidence)
        .map(|f| {
            let relevance = relevance_score(&f, query_text, &entry_points);
            FactNode {
                id: f.id.clone(),
                subject: f.subject.clone(),
                predicate: f.predicate.clone(),
                object: f.object.clone(),
                confidence: f.confidence,
                recorded_at: f.recorded_at.clone(),
                valid_from: f.valid_from.clone(),
                valid_until: f.valid_until.clone(),
                score: relevance,
            }
        })
        .collect();

    nodes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    nodes.truncate(opts.limit);

    for node in &nodes {
        store.touch(&node.id).ok();
    }

    Ok(QueryResult {
        facts: nodes,
        entry_points,
    })
}

fn find_entry_points(facts: &[Fact], query: &str) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
    let mut scores: HashMap<String, f64> = HashMap::new();

    for fact in facts {
        let subject_l = fact.subject.to_lowercase();
        let object_l = fact.object.to_lowercase();
        let pred_l = fact.predicate.to_lowercase();

        let mut score = 0.0;
        for term in &query_terms {
            if subject_l.contains(term) {
                score += 1.0;
            }
            if object_l.contains(term) {
                score += 1.0;
            }
            if pred_l.contains(term) {
                score += 0.5;
            }
        }
        if score > 0.0 {
            *scores.entry(fact.subject.clone()).or_insert(0.0) += score;
            *scores.entry(fact.object.clone()).or_insert(0.0) += score * 0.5;
        }
    }

    let mut sorted: Vec<(String, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(10);
    sorted.into_iter().map(|(s, _)| s).collect()
}

fn relevance_score(fact: &Fact, query: &str, entry_points: &[String]) -> f64 {
    let query_lower = query.to_lowercase();
    let subject_l = fact.subject.to_lowercase();
    let object_l = fact.object.to_lowercase();
    let pred_l = fact.predicate.to_lowercase();

    let mut text_score = 0.0;
    for term in query_lower.split_whitespace() {
        if subject_l.contains(term) {
            text_score += 1.0;
        }
        if object_l.contains(term) {
            text_score += 1.0;
        }
        if pred_l.contains(term) {
            text_score += 0.5;
        }
    }

    let entry_bonus = if entry_points.iter().any(|e| e == &fact.subject) {
        1.0
    } else {
        0.0
    };

    let confidence = fact.confidence;
    text_score * 0.4 + entry_bonus * 0.3 + confidence * 0.3
}
