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
        (Regex::new(r"(?:my name is|i'm|i am|i am) ([a-z][a-z\s]{1,30})").unwrap(), "name", 1.0),
        (Regex::new(r"(?:i use|i'm using|i am using) ([a-z][a-z0-9\s.\-_]{1,40})").unwrap(), "uses", 1.0),
        (Regex::new(r"(?:i work at|i'm at|i am at) ([a-z][a-z0-9\s.\-_]{1,40})").unwrap(), "works_at", 1.0),
        (Regex::new(r"(?:i prefer|i like|i love) ([a-z][a-z0-9\s.\-_]{1,40})").unwrap(), "prefers", 0.6),
        (Regex::new(r"(?:i live in|i'm in|i am in) ([a-z][a-z\s.\-_]{1,40})").unwrap(), "lives_in", 1.0),
        (Regex::new(r"(?:i hate|i can't stand|i dislike) ([a-z][a-z0-9\s.\-_]{1,40})").unwrap(), "dislikes", 0.7),
        (Regex::new(r"(?:my favorite|my favourite) ([a-z][a-z\s]{1,20}) is ([a-z][a-z0-9\s.\-_]{1,40})").unwrap(), "favorite", 0.8),
    ];

    for (re, predicate, confidence) in &patterns {
        for cap in re.captures_iter(&lower) {
            if let Some(g1) = cap.get(1) {
                let object = g1.as_str().trim().to_string();

                let object = if *predicate == "favorite" {
                    if let Some(g2) = cap.get(2) {
                        format!("{}: {}", object, g2.as_str().trim())
                    } else {
                        object
                    }
                } else {
                    object
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
