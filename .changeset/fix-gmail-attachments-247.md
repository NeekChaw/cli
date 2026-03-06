---
"@googleworkspace/cli": patch
---

## fix: support large `--json` bodies and custom `--upload-type` (closes #247)

This release resolves two critical issues when sending large or custom-formatted requests (like Gmail attachments or `.eml` raw messages):

1. **OS Argument Limits**: `--json` now accepts `@<filepath>` to read the JSON request body directly from a file, bypassing OS argument length limits (like macOS `ARG_MAX`). You can also use `--json -` to read from stdin.
2. **Media Uploads Content-Type**: Added an `--upload-type <MIME>` flag to override the default `multipart/related` media upload behavior. When this flag is passed, or when the upload file has an `.eml` extension, `gws` will send the raw file bytes directly with the correct Content-Type (e.g., `message/rfc822`) and pass `uploadType=media` to the API.

### Examples

Read a large JSON body from a file:

```bash
gws gmail users messages send --params '{"userId": "me"}' --json @request.json
```

Send a raw RFC822 `.eml` file directly to Gmail:

```bash
gws gmail users messages send --params '{"userId": "me", "uploadType": "media"}' \
  --upload email.eml
```
