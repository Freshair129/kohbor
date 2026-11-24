use super::knowledge::canonical_bytes;
use super::store::{AiReportMeta, NoteRow, StoredAiReport};
use serde_json::{json, Value};

pub(crate) const PROMPT_VERSION: u32 = 2;
pub(crate) const HISTORY_TOKEN_BUDGET: usize = 12_000;
pub(crate) const HISTORY_ESTIMATED_BYTES_PER_TOKEN: usize = 3;
pub(crate) const MAX_CONTEXT_BYTES: usize = 192 * 1024;

pub(crate) const RULES_V2: &str = r#"You are Kohbor, a match-analysis assistant.
Answer only from this match's knowledge digest, saved analyses provided in context, and results from local read tools. Reply in the user's language, Thai by default.
Unknown values must be written as \"— ไม่ทราบ\" and include the supplied reason. Never estimate missing values. Hero damage, true GPM, draft, and winner are unknown unless the digest says otherwise.
Kills mean hero kills only. There is no win probability. Do not use player names or make claims about people playing the heroes.
Cite times as [07:45](#t=465), events as [deaths#13](#ev=deaths/13), and saved analysis as [title](#doc=id).
When saved analyses or read tools are present, reuse relevant saved analysis first, cite the part reused, and state what is new. Request independent reads together. If tool_support says tools are unavailable, answer from the digest only and say when a detail cannot be looked up.
Return renderable Markdown with short headings, bullets, and tables. Do not output JSON, raw dumps, HTML, or ordinary code blocks. A kohbor-chart fenced block is the only data chart format; use a ref series when available. No Mermaid, images, or external links.
Keep chat answers under about 250 words. For longer answers use write_note only when that tool is listed; otherwise keep the answer concise.
Lane, stomp, and teamfight labels use provisional Kohbor definitions; call them \"เกณฑ์ชั่วคราว\".
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextBlocks {
    pub(crate) rules: String,
    pub(crate) digest: String,
    pub(crate) session: String,
}

pub(crate) fn build_context_blocks(
    digest: &Value,
    viewer: Option<&Value>,
    notes: &[NoteRow],
    report: Option<&StoredAiReport>,
    tools_supported: bool,
) -> Result<ContextBlocks, &'static str> {
    let mut digest_value = digest.clone();
    if let Some(meta) = digest_value.get_mut("meta").and_then(Value::as_object_mut) {
        meta.remove("viewer");
    }
    let digest_bytes = canonical_bytes(&digest_value)?;
    let digest_text = String::from_utf8(digest_bytes).map_err(|_| "context_invalid")?;

    let mut note_index = notes
        .iter()
        .map(|note| {
            json!({
                "id": note.id,
                "title": note.title,
                "bytes": note.bytes,
                "kind": if note.ai { "saved_note" } else { "local_summary" },
            })
        })
        .collect::<Vec<_>>();
    note_index.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    let mut session = if tools_supported {
        json!({
            "viewer": viewer.cloned().unwrap_or_else(|| json!({"own_hero":null,"reason":"no local GSI match or selected own hero"})),
            "saved_analysis": note_index,
            "ai_report": report.map(|report| json!({"id":report.id,"meta":report.meta,"markdown":report.markdown})),
            "tool_support": "local read tools are available"
        })
    } else {
        json!({
            "tool_support": "provider tool calling is unavailable or unverified; answer from the digest only and say when a needed detail cannot be looked up"
        })
    };
    if let Some(object) = session.as_object_mut() {
        object.insert("prompt_version".into(), json!(PROMPT_VERSION));
    }
    let session_text = serde_json::to_string(&session).map_err(|_| "context_invalid")?;
    if digest_text.len().saturating_add(session_text.len()) > MAX_CONTEXT_BYTES {
        return Err("context_too_large");
    }
    Ok(ContextBlocks {
        rules: RULES_V2.to_owned(),
        digest: digest_text,
        session: session_text,
    })
}

pub(crate) fn report_stale_reasons(
    meta: &AiReportMeta,
    bundle_hash: &str,
    knowledge_version: u32,
    own_hero_id: Option<&str>,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if meta.bundle_hash != bundle_hash {
        reasons.push("bundle_changed");
    }
    if meta.knowledge_version != knowledge_version {
        reasons.push("knowledge_changed");
    }
    if meta.prompt_version != PROMPT_VERSION {
        reasons.push("prompt_changed");
    }
    if meta.own_hero_id.as_deref() != own_hero_id {
        reasons.push("own_hero_changed");
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn digest_block_excludes_viewer_ids_paths_and_session_text() {
        let digest = json!({
            "meta":{"viewer":{"own_hero":"npc_dota_hero_axe","match_key":"secret","path":"F:/private"},"duration_s":1800},
            "heroes":[]
        });
        let note = NoteRow {
            id: "secret-note-id".into(),
            title: "private note title".into(),
            ai: false,
            bytes: 17,
        };
        let blocks = build_context_blocks(
            &digest,
            Some(&json!({"own_hero":"npc_dota_hero_axe"})),
            &[note],
            None,
            true,
        )
        .unwrap();
        assert!(!blocks.digest.contains("secret"));
        assert!(!blocks.digest.contains("F:/private"));
        assert!(!blocks.digest.contains("viewer"));
        assert!(blocks.session.contains("npc_dota_hero_axe"));
        let unsupported = build_context_blocks(
            &digest,
            Some(&json!({"own_hero":"npc_dota_hero_axe"})),
            &[NoteRow {
                id: "secret-note-id".into(),
                title: "private note title".into(),
                ai: false,
                bytes: 17,
            }],
            None,
            false,
        )
        .unwrap();
        assert!(unsupported.session.contains("digest only"));
        assert!(!unsupported.session.contains("npc_dota_hero_axe"));
        assert!(!unsupported.session.contains("secret-note-id"));
        assert!(!unsupported.session.contains("private note title"));
    }

    #[test]
    fn digest_block_bytes_do_not_change_when_gsi_history_changes() {
        let digest = json!({
            "knowledge_version":1,
            "meta":{"duration_s":1800,"patch":"fixture"},
            "heroes":[{"id":"npc_dota_hero_axe","final":{"k":4,"d":2,"a":null,"a_reason":"exact assists are not in the bundle"}}]
        });
        let first_viewer = json!({"own_hero":"npc_dota_hero_axe","kda":[4,2,7]});
        let second_viewer = json!({"own_hero":"npc_dota_hero_axe","kda":[4,2,11]});
        let first = build_context_blocks(&digest, Some(&first_viewer), &[], None, true).unwrap();
        let second = build_context_blocks(&digest, Some(&second_viewer), &[], None, true).unwrap();

        assert_eq!(first.digest.as_bytes(), second.digest.as_bytes());
        assert_ne!(first.session, second.session);
        assert!(first.digest.contains("exact assists are not in the bundle"));
        assert!(first.session.contains("7"));
        assert!(second.session.contains("11"));
    }

    #[test]
    fn report_staleness_names_each_changed_input() {
        let meta = AiReportMeta {
            cache_key: "old".into(),
            created_unix_ms: 1,
            model: "model".into(),
            prompt_version: 1,
            knowledge_version: 1,
            bundle_hash: "old".into(),
            own_hero_id: Some("npc_dota_hero_axe".into()),
            usage: Value::Null,
            tool_calls: 0,
            cost_estimate_usd: None,
            cost_estimate_reason: None,
            stale: false,
        };
        assert_eq!(
            report_stale_reasons(&meta, "new", 2, Some("npc_dota_hero_lina")),
            vec![
                "bundle_changed",
                "knowledge_changed",
                "prompt_changed",
                "own_hero_changed"
            ]
        );
    }
}
