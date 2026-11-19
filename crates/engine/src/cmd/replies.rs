//! Commands: canned text (replies kept by name) and a reply drafted by a language model on this
//! computer, both started beside Reply for the user to change or send.

use super::*;

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("canned.list", "Canned text", "Message › Canned text", |s, _| Ok(json!(s.prefs.canned))),
        CommandSpec::new("canned.save", "Save canned text", "Message › Canned text", |s, v| {
            let after = match v.get("after") {
                None | Some(Value::Null) => None,
                Some(after) => Some(
                    serde_json::from_value::<crate::AfterReply>(after.clone())
                        .map_err(|_| CmdError::Params(r#"`after` is "stay", "delete", "archive" or {"move": "<folder>"}"#.into()))?,
                ),
            };
            s.save_canned(p::req_str(v, "name")?, p::req_str(v, "text")?, p::str(v, "was"), after)
        })
        .params(
            r#"{"name": string, "text": string, "was"?: string (the one changed, by its name before; without it a new one is kept), "after"?: "stay" | "delete" | "archive" | {"move": string} (what becomes of the message answered, once the reply is sent)}"#,
        ),
        CommandSpec::new("canned.delete", "Delete canned text", "Message › Canned text", |s, v| s.delete_canned(p::req_str(v, "name")?))
            .params(r#"{"name": string}"#),
        CommandSpec::new("mail.cannedReply", "Reply with canned text", "Message › Canned text", |s, v| {
            s.reply_with_canned(p::str(v, "name"), p::u64(v, "index").and_then(|i| usize::try_from(i).ok()))
        })
        .params(r#"{"name": string} | {"index": number (its place in the list, from 0)}"#)
        .when(has_reading),
        CommandSpec::new("mail.draftReply", "Draft a reply", "Message › Draft a reply", |s, _| s.draft_reply()).when(has_reading),
        CommandSpec::new("assistant.set", "Reply drafting model", "Message › Draft a reply", |s, v| s.set_assistant(p::str(v, "program"), p::str(v, "model")))
            .params(
                r#"{"program"?: string (the path of llama.cpp's llama-server; empty: looked for where TrueVoice keeps its own), "model"?: string (the path of its model file, a .gguf; empty: the same)}"#,
            ),
    ]
}
