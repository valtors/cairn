# Contributing to Cairn

Thanks for your interest in contributing. Cairn is a temporal knowledge store for AI agents, built with Rust and SQLite.

## Ways to Contribute

- **Bug fixes** - Check issues labeled `bug`
- **Features** - Check issues labeled `enhancement` or `good first issue`
- **Extraction patterns** - Add new pattern-based fact extraction rules
- **Temporal queries** - Improve bi-temporal tracking and conflict resolution
- **Federation** - Enhance vector clock sync and peer-to-peer protocols
- **Docs** - Improve README, add examples, write guides
- **Tests** - Add test coverage across crates

## Setup

```bash
git clone https://github.com/valtors/cairn.git
cd cairn
cargo build
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test --workspace

# Run as MCP server
cargo run -- serve
```

## AI Agent Contribution Guide

If you use AI tools to contribute, document which tools you used and which parts they generated. Keep human review in the loop.

## License

By contributing, you agree that your contributions will be licensed under the MIT license.
