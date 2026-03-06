---
"@googleworkspace/cli": patch
---

## fix: `auth export` supports multi-account and `--account` (closes #179)

The `gws auth export` command now correctly handles multi-account
authentication setups. It will export credentials for the default account
by default, and accepts a new `--account EMAIL` argument to export
credentials for a specific account.

Previously, it only checked the legacy single-account `credentials.enc`
file, returning an error for users solely using the multi-account flow.
