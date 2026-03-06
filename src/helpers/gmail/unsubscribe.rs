// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Gmail `+unsubscribe` helper — one-click mailing list unsubscribe via RFC 8058.
//!
//! Scans emails for `List-Unsubscribe` and `List-Unsubscribe-Post` headers,
//! groups candidates by sender, and can execute RFC 8058 one-click unsubscribe
//! POST requests.

use super::*;

/// Parsed unsubscribe information from email headers.
#[derive(Debug, Clone)]
struct UnsubscribeInfo {
    /// The sender's email/name from the `From` header.
    from: String,
    /// The subject of the message (for display).
    subject: String,
    /// HTTPS URL from the `List-Unsubscribe` header, if present.
    https_url: Option<String>,
    /// Mailto address from the `List-Unsubscribe` header, if present.
    mailto: Option<String>,
    /// Whether RFC 8058 one-click unsubscribe is supported
    /// (i.e., `List-Unsubscribe-Post` header is present).
    one_click: bool,
    /// Gmail message ID.
    message_id: String,
}

/// A grouped summary of unsubscribe candidates by sender.
#[derive(Debug)]
struct SenderSummary {
    from: String,
    count: usize,
    one_click: bool,
    https_url: Option<String>,
    mailto: Option<String>,
}

/// Parse the `List-Unsubscribe` header value and extract HTTPS URLs and mailto addresses.
///
/// The header format is a comma-separated list of angle-bracket-delimited URIs:
/// `<https://example.com/unsubscribe>, <mailto:unsub@example.com>`
fn parse_list_unsubscribe(header_value: &str) -> (Option<String>, Option<String>) {
    let mut https_url = None;
    let mut mailto = None;

    for part in header_value.split(',') {
        let trimmed = part.trim();
        // Extract the URI from angle brackets
        let uri = if let (Some(start), Some(end)) = (trimmed.find('<'), trimmed.rfind('>')) {
            &trimmed[start + 1..end]
        } else {
            trimmed
        };

        if uri.starts_with("https://") || uri.starts_with("http://") {
            if https_url.is_none() {
                https_url = Some(uri.to_string());
            }
        } else if uri.starts_with("mailto:") {
            if mailto.is_none() {
                mailto = Some(uri.to_string());
            }
        }
    }

    (https_url, mailto)
}

/// Check if the `List-Unsubscribe-Post` header indicates RFC 8058 support.
fn has_one_click_post(header_value: &str) -> bool {
    header_value
        .to_lowercase()
        .contains("list-unsubscribe=one-click")
}

/// Extract a header value by name from a Gmail message's payload headers.
fn get_header<'a>(headers: &'a [Value], name: &str) -> Option<&'a str> {
    headers.iter().find_map(|h| {
        let h_name = h.get("name")?.as_str()?;
        if h_name.eq_ignore_ascii_case(name) {
            h.get("value")?.as_str()
        } else {
            None
        }
    })
}

/// Handle the `+unsubscribe` subcommand.
pub async fn handle_unsubscribe(matches: &ArgMatches) -> Result<(), GwsError> {
    let max: u32 = matches
        .get_one::<String>("max")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let query = matches
        .get_one::<String>("query")
        .map(|s| s.as_str())
        .unwrap_or("has:unsubscribe");
    let list_mode = matches.get_flag("list");
    let from_filter = matches.get_one::<String>("from");
    let dry_run = matches.get_flag("dry-run");
    let output_format = matches
        .get_one::<String>("format")
        .map(|s| crate::formatter::OutputFormat::from_str(s))
        .unwrap_or(crate::formatter::OutputFormat::Table);

    if !list_mode && from_filter.is_none() {
        return Err(GwsError::Validation(
            "Specify --list to scan for candidates, or --from <sender> to unsubscribe.\n\
             Examples:\n  \
               gws gmail +unsubscribe --list\n  \
               gws gmail +unsubscribe --from \"noreply@example.com\""
                .to_string(),
        ));
    }

    // Authenticate
    let token = auth::get_token(&[GMAIL_SCOPE], None)
        .await
        .map_err(|e| GwsError::Auth(format!("Gmail auth failed: {e}")))?;

    let client = crate::client::build_client()?;

    // 1. List message IDs matching the query
    let list_url = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
    let effective_query = if let Some(sender) = from_filter {
        format!("from:{sender} has:unsubscribe")
    } else {
        query.to_string()
    };

    let list_resp = client
        .get(list_url)
        .query(&[("q", &effective_query), ("maxResults", &max.to_string())])
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| GwsError::Other(anyhow::anyhow!("Failed to list messages: {e}")))?;

    if !list_resp.status().is_success() {
        let err = list_resp.text().await.unwrap_or_default();
        return Err(GwsError::Api {
            code: 0,
            message: err,
            reason: "list_failed".to_string(),
            enable_url: None,
        });
    }

    let list_json: Value = list_resp
        .json()
        .await
        .map_err(|e| GwsError::Other(anyhow::anyhow!("Failed to parse list response: {e}")))?;

    let messages = match list_json.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => {
            println!("No messages found with unsubscribe headers.");
            return Ok(());
        }
    };

    if messages.is_empty() {
        println!("No messages found with unsubscribe headers.");
        return Ok(());
    }

    // 2. Fetch metadata for each message (concurrently)
    use futures_util::stream::{self, StreamExt};

    let msg_ids: Vec<String> = messages
        .iter()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    let infos: Vec<UnsubscribeInfo> = stream::iter(msg_ids)
        .map(|msg_id| {
            let client = &client;
            let token = &token;
            async move {
                let get_url = format!(
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?\
                     format=metadata\
                     &metadataHeaders=From\
                     &metadataHeaders=Subject\
                     &metadataHeaders=List-Unsubscribe\
                     &metadataHeaders=List-Unsubscribe-Post",
                    crate::validate::encode_path_segment(&msg_id)
                );

                let resp = crate::client::send_with_retry(|| {
                    client.get(&get_url).bearer_auth(token)
                })
                .await
                .ok()?;

                if !resp.status().is_success() {
                    return None;
                }

                let msg: Value = resp.json().await.ok()?;
                let headers = msg
                    .get("payload")
                    .and_then(|p| p.get("headers"))
                    .and_then(|h| h.as_array())?;

                let list_unsub = get_header(headers, "List-Unsubscribe")?;
                let (https_url, mailto) = parse_list_unsubscribe(list_unsub);

                // Only include if there's at least one unsubscribe mechanism
                if https_url.is_none() && mailto.is_none() {
                    return None;
                }

                let one_click = get_header(headers, "List-Unsubscribe-Post")
                    .map(has_one_click_post)
                    .unwrap_or(false);

                Some(UnsubscribeInfo {
                    from: get_header(headers, "From").unwrap_or("").to_string(),
                    subject: get_header(headers, "Subject").unwrap_or("").to_string(),
                    https_url,
                    mailto,
                    one_click,
                    message_id: msg_id,
                })
            }
        })
        .buffer_unordered(10)
        .filter_map(|r| async { r })
        .collect()
        .await;

    if infos.is_empty() {
        println!("No messages with List-Unsubscribe headers found.");
        return Ok(());
    }

    // 3a. List mode — group by sender and display
    if list_mode {
        let summaries = group_by_sender(&infos);

        let output_items: Vec<Value> = summaries
            .iter()
            .map(|s| {
                let mut entry = json!({
                    "from": s.from,
                    "count": s.count,
                    "oneClick": s.one_click,
                });
                if let Some(url) = &s.https_url {
                    entry["unsubscribeUrl"] = json!(url);
                }
                if let Some(mailto) = &s.mailto {
                    entry["mailto"] = json!(mailto);
                }
                entry
            })
            .collect();

        let output = json!({
            "candidates": output_items,
            "total": summaries.len(),
            "query": effective_query,
        });

        println!(
            "{}",
            crate::formatter::format_value(&output, &output_format)
        );
        return Ok(());
    }

    // 3b. Unsubscribe mode — execute for the --from sender
    let first = &infos[0];

    if first.one_click {
        if let Some(url) = &first.https_url {
            if dry_run {
                let output = json!({
                    "action": "one-click unsubscribe (RFC 8058)",
                    "dry_run": true,
                    "from": first.from,
                    "url": url,
                    "method": "POST",
                    "body": "List-Unsubscribe=One-Click",
                });
                println!(
                    "{}",
                    crate::formatter::format_value(&output, &output_format)
                );
                return Ok(());
            }

            // Execute RFC 8058 one-click unsubscribe
            eprintln!(
                "📧 Unsubscribing from: {}",
                first.from
            );
            eprintln!("   Method: RFC 8058 one-click POST");

            let unsub_resp = client
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await
                .map_err(|e| {
                    GwsError::Other(anyhow::anyhow!("Unsubscribe request failed: {e}"))
                })?;

            let status = unsub_resp.status();
            let body = unsub_resp.text().await.unwrap_or_default();

            let result = json!({
                "action": "one-click unsubscribe (RFC 8058)",
                "from": first.from,
                "url": url,
                "status": status.as_u16(),
                "success": status.is_success() || status.as_u16() == 302,
                "message_id": first.message_id,
            });

            if !status.is_success() && status.as_u16() != 302 {
                eprintln!(
                    "   ⚠ Server returned HTTP {}: {}",
                    status.as_u16(),
                    body.chars().take(200).collect::<String>()
                );
            } else {
                eprintln!("   ✅ Unsubscribe request sent successfully");
            }

            println!(
                "{}",
                crate::formatter::format_value(&result, &output_format)
            );
            return Ok(());
        }
    }

    // Fallback — no RFC 8058 support, show available options
    let mut fallback = json!({
        "action": "manual unsubscribe required",
        "from": first.from,
        "oneClick": false,
        "reason": "This sender does not support RFC 8058 one-click unsubscribe",
    });

    if let Some(url) = &first.https_url {
        fallback["unsubscribeUrl"] = json!(url);
        fallback["hint"] = json!("Open this URL in a browser to unsubscribe");
    }
    if let Some(mailto) = &first.mailto {
        fallback["mailto"] = json!(mailto);
        fallback["hint"] =
            json!("Send an email to this address to unsubscribe");
    }

    println!(
        "{}",
        crate::formatter::format_value(&fallback, &output_format)
    );

    Ok(())
}

/// Group unsubscribe infos by sender (normalized email address).
fn group_by_sender(infos: &[UnsubscribeInfo]) -> Vec<SenderSummary> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, SenderSummary> = BTreeMap::new();

    for info in infos {
        let key = normalize_sender(&info.from);
        let entry = groups.entry(key).or_insert_with(|| SenderSummary {
            from: info.from.clone(),
            count: 0,
            one_click: info.one_click,
            https_url: info.https_url.clone(),
            mailto: info.mailto.clone(),
        });
        entry.count += 1;
        // If any message supports one-click, mark the group as supporting it
        if info.one_click {
            entry.one_click = true;
            if entry.https_url.is_none() {
                entry.https_url = info.https_url.clone();
            }
        }
    }

    let mut summaries: Vec<SenderSummary> = groups.into_values().collect();
    summaries.sort_by(|a, b| b.count.cmp(&a.count));
    summaries
}

/// Normalize a sender string for grouping.
/// Extracts the email part from formats like `"Name <email@example.com>"`.
fn normalize_sender(from: &str) -> String {
    if let Some(start) = from.rfind('<') {
        if let Some(end) = from[start..].find('>') {
            return from[start + 1..start + end].to_lowercase();
        }
    }
    from.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_list_unsubscribe_https_and_mailto() {
        let (url, mailto) = parse_list_unsubscribe(
            "<https://example.com/unsub?id=123>, <mailto:unsub@example.com>",
        );
        assert_eq!(url.unwrap(), "https://example.com/unsub?id=123");
        assert_eq!(mailto.unwrap(), "mailto:unsub@example.com");
    }

    #[test]
    fn test_parse_list_unsubscribe_https_only() {
        let (url, mailto) =
            parse_list_unsubscribe("<https://example.com/unsubscribe>");
        assert_eq!(url.unwrap(), "https://example.com/unsubscribe");
        assert!(mailto.is_none());
    }

    #[test]
    fn test_parse_list_unsubscribe_mailto_only() {
        let (url, mailto) =
            parse_list_unsubscribe("<mailto:leave@example.com>");
        assert!(url.is_none());
        assert_eq!(mailto.unwrap(), "mailto:leave@example.com");
    }

    #[test]
    fn test_parse_list_unsubscribe_empty() {
        let (url, mailto) = parse_list_unsubscribe("");
        assert!(url.is_none());
        assert!(mailto.is_none());
    }

    #[test]
    fn test_has_one_click_post() {
        assert!(has_one_click_post("List-Unsubscribe=One-Click"));
        assert!(has_one_click_post("list-unsubscribe=one-click"));
        assert!(!has_one_click_post(""));
        assert!(!has_one_click_post("something-else"));
    }

    #[test]
    fn test_normalize_sender() {
        assert_eq!(
            normalize_sender("Newsletter <news@example.com>"),
            "news@example.com"
        );
        assert_eq!(normalize_sender("plain@example.com"), "plain@example.com");
        assert_eq!(
            normalize_sender("\"Name\" <NAME@EXAMPLE.COM>"),
            "name@example.com"
        );
    }

    #[test]
    fn test_group_by_sender() {
        let infos = vec![
            UnsubscribeInfo {
                from: "News <a@example.com>".to_string(),
                subject: "Subject 1".to_string(),
                https_url: Some("https://example.com/unsub".to_string()),
                mailto: None,
                one_click: true,
                message_id: "1".to_string(),
            },
            UnsubscribeInfo {
                from: "News <a@example.com>".to_string(),
                subject: "Subject 2".to_string(),
                https_url: Some("https://example.com/unsub".to_string()),
                mailto: None,
                one_click: true,
                message_id: "2".to_string(),
            },
            UnsubscribeInfo {
                from: "Other <b@example.com>".to_string(),
                subject: "Subject 3".to_string(),
                https_url: None,
                mailto: Some("mailto:unsub@example.com".to_string()),
                one_click: false,
                message_id: "3".to_string(),
            },
        ];

        let groups = group_by_sender(&infos);
        assert_eq!(groups.len(), 2);
        // First should be a@example.com with count=2 (sorted by count desc)
        assert_eq!(groups[0].count, 2);
        assert!(groups[0].one_click);
        assert_eq!(groups[1].count, 1);
        assert!(!groups[1].one_click);
    }
}
