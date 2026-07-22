use clap::{Parser, Subcommand};
use recall_mcp::{dispatch, list_tools};
use recall_store::{RememberOptions, Store};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "recall", version, about = "local-first memory for AI agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(long, env = "RECALL_DB", default_value = "~/.recall/memory.db")]
    db: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    Remember {
        #[arg(long)]
        subject: String,
        #[arg(long)]
        predicate: String,
        #[arg(long)]
        object: String,
        #[arg(long)]
        confidence: Option<f64>,
        #[arg(long)]
        source: Option<String>,
    },
    Recall {
        query: String,
        #[arg(long, default_value = "2")]
        depth: usize,
        #[arg(long, default_value = "50")]
        limit: usize,
        #[arg(long)]
        as_of: Option<String>,
    },
    Forget {
        #[arg(long, default_value = "30")]
        older_than_days: i64,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
    Export,
    Import {
        #[arg(long)]
        file: PathBuf,
    },
    Extract {
        text: String,
        #[arg(long)]
        user_name: Option<String>,
    },
    Serve,
}

fn expand_path(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy().to_string();
    if s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(s.trim_start_matches("~/"));
        }
    }
    p
}

fn main() {
    let cli = Cli::parse();
    let db_path = expand_path(cli.db.clone());

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let store = match Store::open(&db_path, None) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error opening store: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Remember { subject, predicate, object, confidence, source } => {
            let opts = RememberOptions {
                valid_from: None,
                confidence,
                source: source.map(|s| s.to_string()),
                device_id: None,
            };
            match store.remember(&subject, &predicate, &object, opts) {
                Ok(id) => println!("remembered: {}", id),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Recall { query, depth, limit, as_of } => {
            let args = json!({
                "query": query,
                "depth": depth,
                "limit": limit,
                "as_of": as_of,
            });
            match dispatch(&store, "recall", &args) {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Forget { older_than_days, dry_run, force } => {
            let args = json!({
                "older_than_days": older_than_days,
                "dry_run": dry_run,
                "force": force,
            });
            match dispatch(&store, "forget", &args) {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Export => {
            match dispatch(&store, "export_memory", &json!({})) {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Import { file } => {
            let content = match std::fs::read_to_string(&file) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error reading file: {}", e);
                    std::process::exit(1);
                }
            };
            let data: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| {
                json!({ "facts": serde_json::from_str::<serde_json::Value>(&content).unwrap_or(json!([])) })
            });
            match dispatch(&store, "import_memory", &data) {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Extract { text, user_name } => {
            let args = json!({
                "text": text,
                "user_name": user_name,
            });
            match dispatch(&store, "extract", &args) {
                Ok(result) => println!("{}", serde_json::to_string_pretty(&result).unwrap()),
                Err(e) => eprintln!("error: {}", e),
            }
        }
        Commands::Serve => {
            run_mcp_server(&store);
        }
    }
}

fn run_mcp_server(store: &Store) {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                let _ = writeln!(stdout, "{}", json!({"error": "invalid json"}));
                continue;
            }
        };

        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(json!(null));

        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "recall", "version": "0.1.0" }
                }
            }),
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": list_tools() }
            }),
            "tools/call" => {
                let tool = request.get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args = request.get("params")
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(json!({}));

                match dispatch(store, tool, &args) {
                    Ok(result) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap() }]
                        }
                    }),
                    Err(e) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32603, "message": e }
                    }),
                }
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("unknown method: {}", method) }
            }),
        };

        let _ = writeln!(stdout, "{}", response);
        let _ = stdout.flush();
    }
}
