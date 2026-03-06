---
name: gws-gmail-unsubscribe
version: 1.0.0
description: "Gmail: One-click mailing list unsubscribe (RFC 8058)."
metadata:
  openclaw:
    category: "productivity"
    requires:
      bins: ["gws"]
    cliHelp: "gws gmail +unsubscribe --help"
---

# gmail +unsubscribe

> **PREREQUISITE:** Read `../gws-shared/SKILL.md` for auth, global flags, and security rules. If missing, run `gws generate-skills` to create it.

One-click mailing list unsubscribe (RFC 8058)

## Usage

```bash
gws gmail +unsubscribe
```

## Flags

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--list` | — | — | List unsubscribe candidates grouped by sender |
| `--from` | — | — | Unsubscribe from this sender |
| `--max` | — | 50 | Maximum messages to scan (default: 50) |
| `--query` | — | — | Gmail search query (default: has:unsubscribe) |
| `--dry-run` | — | — | Show what would be done without executing |

## Examples

```bash
gws gmail +unsubscribe --list
gws gmail +unsubscribe --list --max 200
gws gmail +unsubscribe --from 'noreply@example.com'
gws gmail +unsubscribe --from 'noreply@example.com' --dry-run
```

## Tips

- Uses RFC 8058 one-click unsubscribe when available.
- Falls back to showing mailto/URL for manual unsubscribe.

## See Also

- [gws-shared](../gws-shared/SKILL.md) — Global flags and auth
- [gws-gmail](../gws-gmail/SKILL.md) — All send, read, and manage email commands
