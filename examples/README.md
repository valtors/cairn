# cairn examples

## claude desktop

add cairn as an MCP server in your claude desktop config:

```json
{
  "mcpServers": {
    "cairn": {
      "command": "cairn",
      "args": ["serve"]
    }
  }
}
```

then ask your agent to remember things:

- "remember that i use arch linux"
- "remember that my project uses postgres not mysql"
- "what do you know about my setup?"
- "what did we know about my setup last month?"
- "forget everything older than 30 days"

## cli session

```bash
# store facts
cairn remember --subject alice --predicate works_on --object "auth service"
cairn remember --subject bob --predicate reports_to --object alice
cairn remember --subject alice --predicate uses_os --object linux

# query
cairn recall "what do you know about alice"
cairn recall "who reports to alice"

# temporal query - what was true in the past
cairn recall --as-of 2024-06-01 "what os does alice use"

# forgetting
cairn forget --older-than 30d
cairn forget --subject alice --predicate uses_os

# export and sync
cairn export > laptop-memory.json
cairn import < laptop-memory.json

# sync with another device
cairn sync --peer http://192.168.1.100:4321
```

## two-device sync

on laptop:
```bash
cairn serve --port 4321
```

on phone:
```bash
cairn sync --peer http://laptop.local:4321
```

both devices now share the same brain. vector clocks handle conflicts. no server needed.
