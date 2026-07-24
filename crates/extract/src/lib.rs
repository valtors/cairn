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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_name() {
        let facts = extract_from_text("my name is tamish", Some("user"));
        assert!(facts
            .iter()
            .any(|f| f.predicate == "name" && f.object == "tamish"));
    }

    #[test]
    fn extract_uses() {
        let facts = extract_from_text("i use linux", Some("user"));
        assert!(facts
            .iter()
            .any(|f| f.predicate == "uses" && f.object == "linux"));
    }

    #[test]
    fn extract_works_at() {
        let facts = extract_from_text("i work at valtors", Some("user"));
        assert!(facts
            .iter()
            .any(|f| f.predicate == "works_at" && f.object == "valtors"));
    }

    #[test]
    fn extract_lives_in() {
        let facts = extract_from_text("i live in bangalore", Some("user"));
        assert!(facts
            .iter()
            .any(|f| f.predicate == "lives_in" && f.object == "bangalore"));
    }

    #[test]
    fn extract_dislikes() {
        let facts = extract_from_text("i hate bugs", Some("user"));
        assert!(facts
            .iter()
            .any(|f| f.predicate == "dislikes" && f.object == "bugs"));
    }

    #[test]
    fn extract_prefers_lower_confidence() {
        let facts = extract_from_text("i prefer vim", Some("user"));
        let pref = facts.iter().find(|f| f.predicate == "prefers");
        assert!(pref.is_some());
        assert_eq!(pref.unwrap().confidence, 0.6);
    }

    #[test]
    fn extract_default_subject() {
        let facts = extract_from_text("i use rust", None);
        assert!(facts.iter().all(|f| f.subject == "user"));
    }

    #[test]
    fn extract_empty_text() {
        let facts = extract_from_text("", Some("user"));
        assert!(facts.is_empty());
    }

    #[test]
    fn extract_no_match() {
        let facts = extract_from_text("the weather is nice today", Some("user"));
        assert!(facts.is_empty());
    }

    #[test]
    fn trim_at_stop_words_basic() {
        let result = trim_at_stop_words("linux and windows", &["and", "but", "or"]);
        assert_eq!(result, "linux");
    }

    #[test]
    fn trim_at_stop_words_no_stop() {
        let result = trim_at_stop_words("rust go", &["and", "but"]);
        assert_eq!(result, "rust go");
    }

    #[test]
    fn extract_multiple_facts() {
        let facts = extract_from_text(
            "my name is damir and i use linux and i live in bangalore",
            Some("user"),
        );
        assert!(facts.len() >= 2);
    }
}
