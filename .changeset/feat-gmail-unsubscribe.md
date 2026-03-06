---
"@googleworkspace/cli": minor
---

## feat: add Gmail `+unsubscribe` skill (closes #114)

New `gws gmail +unsubscribe` helper that implements RFC 8058 one-click
mailing list unsubscribe.

### Usage

```bash
# List unsubscribe candidates grouped by sender
gws gmail +unsubscribe --list

# Unsubscribe from a specific sender (RFC 8058 one-click POST)
gws gmail +unsubscribe --from "noreply@example.com"

# Dry run — show what would be done
gws gmail +unsubscribe --from "noreply@example.com" --dry-run
```

### How it works

1. **`--list`** scans recent emails for `List-Unsubscribe` and
   `List-Unsubscribe-Post` headers, groups by sender, and shows
   count + whether RFC 8058 one-click is supported.
2. **`--from`** finds the latest message from that sender and:
   - If RFC 8058 supported → sends `POST` with body
     `List-Unsubscribe=One-Click` to the HTTPS unsubscribe URL.
   - Otherwise → shows the mailto address or URL for manual action.
