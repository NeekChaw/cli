---
"@googleworkspace/cli": patch
---

## fix: add Chat API scope to all scope presets (closes #236)

`gws auth login -s chat` previously did not include any Google Chat API
OAuth scope, causing Chat API calls to fail with "insufficient
authentication scopes".

### Changes

- Added `chat.messages` to `MINIMAL_SCOPES` and `FULL_SCOPES`
- Added `chat.spaces.readonly` to `READONLY_SCOPES`

The `scope_matches_service` filter already correctly maps the `chat`
service name to scopes starting with `chat.`, so only the missing scope
entries needed to be added.
