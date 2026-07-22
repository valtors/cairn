use recall_store::Fact;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

pub struct TraversalResult {
    pub facts: Vec<Fact>,
    pub hops: Vec<(String, String)>,
}

pub fn traverse(
    conn: &Connection,
    entry_subjects: &[String],
    max_depth: usize,
    as_of: Option<&str>,
) -> Result<TraversalResult, String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut facts: Vec<Fact> = Vec::new();
    let mut hops: Vec<(String, String)> = Vec::new();
    let mut frontier: Vec<String> = entry_subjects.to_vec();

    for _ in 0..max_depth {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier: Vec<String> = Vec::new();

        for subject in &frontier {
            if visited.contains(subject) {
                continue;
            }
            visited.insert(subject.clone());

            let found: Vec<Fact> = match as_of {
                Some(ts) => {
                    let mut stmt = conn
                        .prepare("SELECT * FROM facts WHERE subject = ? AND tombstone = 0 AND recorded_at <= ? AND (valid_until IS NULL OR valid_until > ?) ORDER BY recorded_at DESC")
                        .map_err(|e| e.to_string())?;
                    let rows: Vec<Fact> = stmt
                        .query_map(params![subject, ts, ts], row_to_fact)
                        .map_err(|e| e.to_string())?
                        .filter_map(|r| r.ok())
                        .collect();
                    rows
                }
                None => {
                    let mut stmt = conn
                        .prepare("SELECT * FROM facts WHERE subject = ? AND tombstone = 0 AND valid_until IS NULL ORDER BY recorded_at DESC")
                        .map_err(|e| e.to_string())?;
                    let rows: Vec<Fact> = stmt
                        .query_map(params![subject], row_to_fact)
                        .map_err(|e| e.to_string())?
                        .filter_map(|r| r.ok())
                        .collect();
                    rows
                }
            };

            for fact in found {
                hops.push((fact.subject.clone(), fact.predicate.clone()));
                next_frontier.push(fact.object.clone());
                facts.push(fact);
            }
        }
        frontier = next_frontier;
    }

    let mut seen: HashMap<String, Fact> = HashMap::new();
    for fact in facts {
        seen.entry(fact.id.clone()).or_insert(fact);
    }
    let mut facts: Vec<Fact> = seen.into_values().collect();
    facts.sort_by(|a, b| b.recorded_at.cmp(&a.recorded_at));

    Ok(TraversalResult { facts, hops })
}

fn row_to_fact(row: &rusqlite::Row) -> rusqlite::Result<Fact> {
    Ok(Fact {
        id: row.get(0)?,
        subject: row.get(1)?,
        predicate: row.get(2)?,
        object: row.get(3)?,
        valid_from: row.get(4)?,
        valid_until: row.get(5)?,
        recorded_at: row.get(6)?,
        confidence: row.get(7)?,
        source: row.get(8)?,
        tombstone: row.get::<_, i64>(9)? != 0,
        tombstone_reason: row.get(10)?,
        access_count: row.get(11)?,
        last_accessed: row.get(12)?,
        device_id: row.get(13)?,
        vector_clock: row.get(14)?,
    })
}
