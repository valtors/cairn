//! pattern-based fact extraction. zero LLM calls.
//!
//! the agent calls `remember()` during its normal response. but sometimes
//! the agent doesn't call it. so a post-turn hook runs these regex patterns
//! over the conversation text and extracts the obvious stuff.
//!
//! 80% of facts are "my name is X", "i use Y", "i work at Z". a regex catches
//! those for free. the other 20% need the agent's judgment. that's fine.

use regex::Regex;

pub struct ExtractedFact {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f64,
    pub source: String,
}

pub fn extract_from_text(text: &str, user_name: Option<&str>) -> Vec<ExtractedFact> {
    let subject = user_name.unwrap_or("user").to_string();
    let lower = text.to_lowercase();
    let mut facts = Vec::new();

    let patterns: Vec<(Regex, &str, f64)> = vec![
        (Regex::new(r"(?:my name is|i'm|i am) ([a-z][a-z]+(?: [a-z]+)*)").unwrap(), "name", 1.0),
        (Regex::new(r"(?:i use|i'm using|i am using) ([a-z][a-z0-9]+(?:[.\-_ ][a-z0-9]+)*)").unwrap(), "uses", 1.0),
        (Regex::new(r"(?:i work at|i'm at|i am at) ([a-z][a-z0-9]+(?: [a-z0-9]+)*)").unwrap(), "works_at", 1.0),
        (Regex::new(r"(?:i prefer|i like|i love) ([a-z][a-z0-9]+(?:[.\-_ ][a-z0-9]+)*)").unwrap(), "prefers", 0.6),
        (Regex::new(r"(?:i live in|i'm in|i am in) ([a-z][a-z]+(?: [a-z]+)*)").unwrap(), "lives_in", 1.0),
        (Regex::new(r"(?:i hate|i can't stand|i dislike) ([a-z][a-z0-9]+(?:[.\-_ ][a-z0-9]+)*)").unwrap(), "dislikes", 0.7),
        (Regex::new(r"(?:my favorite|my favourite) ([a-z][a-z]+) is ([a-z][a-z0-9]+(?:[.\-_ ][a-z0-9]+)*)").unwrap(), "favorite", 0.8),
    ];

    let stop_words = ["and", "but", "or", "so", "because", "i", "my", "we", "they"];

    for (re, predicate, confidence) in &patterns {
        for cap in re.captures_iter(&lower) {
            if let Some(g1) = cap.get(1) {
                let raw = g1.as_str().trim().to_string();

                let object = if *predicate == "favorite" {
                    if let Some(g2) = cap.get(2) {
                        format!("{}: {}", raw, g2.as_str().trim())
                    } else {
                        raw
                    }
                } else {
                    trim_at_stop_words(&raw, &stop_words)
                };

                if !object.is_empty() && object.len() < 50 {
                    facts.push(ExtractedFact {
                        subject: subject.clone(),
                        predicate: predicate.to_string(),
                        object,
                        confidence: *confidence,
                        source: "inferred".to_string(),
                    });
                }
            }
        }
    }

    facts
}

fn trim_at_stop_words(s: &str, stop_words: &[&str]) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut end = words.len();
    for (i, w) in words.iter().enumerate() {
        if stop_words.contains(w) {
            end = i;
            break;
        }
    }
    words[..end].join(" ")
}
