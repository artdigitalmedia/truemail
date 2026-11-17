//! Commands another program tells the running MailCraft to run.
//!
//! A launch on a data folder whose MailCraft already runs hands its `--cmd` commands to that
//! window (`single_instance.rs`), which runs them and says how each went. A voice-control or
//! switch-access program drives the window that way, for someone who can't use a keyboard or a
//! mouse: `mailcraft --cmd list.next --cmd mail.reply` moves to the next message and starts a
//! reply in the window already open, and prints what the window answered.
//!
//! Only the commands in [`ALLOWED`] are run for another program: moving about, reading, marking,
//! starting and writing a message, archiving. Nothing that sends or deletes mail, moves it
//! elsewhere, reads or writes a file, or changes an account or a setting: a word misheard must
//! never send or lose anything, so those stay with the person at the window. (The first
//! MailCraft still runs any `--cmd` it's started with, as before: `desktop/start.rs`.)

use mailcraft_ui_egui::MailApp;
use serde_json::{Value, json};

/// Most commands one request tells; a request with more is refused.
pub const MAX_TOLD: usize = 16;
/// Longest command id read (the longest there is has under forty letters).
const MAX_ID: usize = 64;
/// Longest answer said with the commands' results in it; past it, the results are left out.
const MAX_SAID: usize = 512 * 1024;

/// A command to run, as `--cmd ID[=JSON]` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Told {
    pub id: String,
    /// The parameters, as JSON text (`{}` when none were given); read when the command is run.
    pub params: String,
}

/// The commands run for another program. Each undoes easily or changes nothing: see the top of
/// this file before adding one.
const ALLOWED: [&str; 34] = [
    // Moving about.
    "list.next",
    "list.prev",
    "list.first",
    "list.last",
    "list.pageDown",
    "list.pageUp",
    "list.open",
    "list.select",
    "folder.open",
    "folder.unified",
    "view.mail",
    "view.calendar",
    "view.people",
    "view.tasks",
    "view.notes",
    "search.run",
    "search.clear",
    // Reading: what's listed and what the open message says, for a program that reads it aloud.
    "list.messages",
    "message.read",
    // Marking.
    "mail.markRead",
    "mail.markUnread",
    "mail.toggleRead",
    "mail.flag",
    "mail.pin",
    // Starting and writing a message (sending it stays with the person at the window).
    "mail.new",
    "mail.reply",
    "mail.replyAll",
    "mail.forward",
    "compose.update",
    "compose.saveDraft",
    "compose.activate",
    "compose.list",
    // Filing, and taking it back.
    "mail.archive",
    "edit.undo",
];

/// Whether `id` is run for another program.
pub fn allowed(id: &str) -> bool {
    ALLOWED.contains(&id)
}

/// The commands a launch was started with (`--cmd ID[=JSON]`, in order; those past
/// [`MAX_TOLD`] left out, as a request with more is refused whole).
pub fn from_args(args: &[String]) -> Vec<Told> {
    args.windows(2)
        .filter(|w| w.first().is_some_and(|a| a == "--cmd"))
        .filter_map(|w| w.get(1))
        .map(|c| {
            let (id, params) = c.split_once('=').unwrap_or((c.as_str(), "{}"));
            Told { id: id.to_string(), params: params.to_string() }
        })
        .take(MAX_TOLD)
        .collect()
}

/// The commands as a request carries them: `[{"id": …, "params": "<JSON text>"}]`.
pub fn to_json(told: &[Told]) -> Value {
    Value::Array(told.iter().map(|t| json!({ "id": t.id, "params": t.params })).collect())
}

/// The commands a request carries, or `None` when its `run` isn't a list of at most
/// [`MAX_TOLD`] objects, each with an `id` (a string, not empty, at most [`MAX_ID`] long) and,
/// if it has them, `params`: JSON text as a launch sends it, or the JSON itself.
pub fn from_json(list: &Value) -> Option<Vec<Told>> {
    let list = list.as_array()?;
    if list.len() > MAX_TOLD {
        return None;
    }
    list.iter()
        .map(|one| {
            let one = one.as_object()?;
            let id = one.get("id")?.as_str()?;
            if id.is_empty() || id.len() > MAX_ID {
                return None;
            }
            let params = match one.get("params") {
                None | Some(Value::Null) => "{}".to_string(),
                Some(Value::String(text)) => text.clone(),
                Some(json) => json.to_string(),
            };
            Some(Told { id: id.to_string(), params })
        })
        .collect()
}

/// Run `told` in the window, in order, and say how each went, on one line:
/// `{"ran": [{"id": …, "ok": true, "result"?: …} | {"id": …, "ok": false, "error": …}]}`.
/// After one that doesn't run, the rest aren't either: a reply told after a move that failed
/// would answer another message.
pub fn run(app: &mut MailApp, told: &[Told]) -> String {
    let mut failed = false;
    let mut ran: Vec<Value> = Vec::with_capacity(told.len());
    for t in told {
        let went = if failed { Err("not run: the command before it wasn't".to_string()) } else { run_one(app, t) };
        failed |= went.is_err();
        ran.push(outcome(&t.id, went));
    }
    let said = json!({ "ran": ran }).to_string();
    if said.len() <= MAX_SAID {
        return said;
    }
    // Too much to say on the line (a long list, a very long message): how each went, without
    // what it answered.
    for one in &mut ran {
        if let Some(one) = one.as_object_mut()
            && one.remove("result").is_some()
        {
            one.insert("resultLeftOut".into(), Value::Bool(true));
        }
    }
    json!({ "ran": ran }).to_string()
}

/// Run one command, if it's one run for another program and its parameters are JSON.
fn run_one(app: &mut MailApp, told: &Told) -> Result<Value, String> {
    if !allowed(&told.id) {
        return Err(format!("`{}` isn't run for another program", told.id));
    }
    let params: Value = serde_json::from_str(&told.params).map_err(|e| format!("bad JSON: {e}"))?;
    app.run_told(&told.id, params)
}

/// How one command went, as [`run`] says it.
fn outcome(id: &str, went: Result<Value, String>) -> Value {
    match went {
        Ok(Value::Null) => json!({ "id": id, "ok": true }),
        Ok(result) => json!({ "id": id, "ok": true, "result": result }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailcraft_engine::Session;
    use mailcraft_engine::model::dates::Clock;

    fn app() -> MailApp {
        let session = Session::open(None, Clock::new(1_700_000_000, 0));
        MailApp::new(session, Box::new(mailcraft_ui_egui::NullBackend), Box::new(mailcraft_ui_egui::NullServices(1_700_000_000)))
    }

    fn told(id: &str, params: &str) -> Told {
        Told { id: id.to_string(), params: params.to_string() }
    }

    fn ran(said: &str) -> Vec<Value> {
        let said: Value = serde_json::from_str(said).unwrap();
        said["ran"].as_array().unwrap().clone()
    }

    /// Every command run for another program is one MailCraft has: a renamed or removed one
    /// would be refused as unknown only when someone asked for it.
    #[test]
    fn every_allowed_command_exists() {
        let registry = Session::blank().registry();
        for id in ALLOWED {
            assert!(registry.get(id).is_some(), "{id} isn't a command");
        }
        let mut sorted = ALLOWED.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ALLOWED.len(), "one is listed twice");
    }

    /// Nothing that sends, deletes, moves mail away, touches a file or changes an account is
    /// run for another program.
    #[test]
    fn what_sends_or_loses_mail_is_refused() {
        for id in [
            "compose.send",
            "outbox.send",
            "mail.delete",
            "mail.move",
            "mail.junk",
            "folder.delete",
            "folder.empty",
            "compose.attach",
            "attachment.save",
            "message.saveAs",
            "account.remove",
            "account.setPassword",
            "rules.add",
            "message.showPictures",
            "ui.addAccount",
            "",
        ] {
            assert!(!allowed(id), "{id}");
        }
        let mut a = app();
        let said = ran(&run(&mut a, &[told("compose.send", "{}")]));
        assert_eq!(said[0]["ok"], false);
        assert!(said[0]["error"].as_str().unwrap().contains("isn't run for another program"), "{said:?}");
    }

    /// `--cmd ID[=JSON]` is read as the first MailCraft reads it, and crosses a request whole.
    #[test]
    fn commands_cross_from_a_launch_to_the_window() {
        let args: Vec<String> = ["--sample", "--cmd", "list.next", "--dark", "--cmd", "mail.new={\"to\":\"Ana <ana@x.example>\"}", "--cmd"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        let from = from_args(&args);
        assert_eq!(from, [told("list.next", "{}"), told("mail.new", "{\"to\":\"Ana <ana@x.example>\"}")]);
        assert_eq!(from_json(&to_json(&from)), Some(from));
        assert!(from_args(&["--data".to_string(), "D:/Mail".to_string()]).is_empty());
        let many: Vec<String> = (0..MAX_TOLD + 4).flat_map(|_| ["--cmd".to_string(), "list.next".to_string()]).collect();
        assert_eq!(from_args(&many).len(), MAX_TOLD);
    }

    /// A request's `run` is read strictly: the JSON itself or its text as `params`, and anything
    /// else refused.
    #[test]
    fn a_request_s_commands_are_read_strictly() {
        let read = |text: &str| from_json(&serde_json::from_str(text).unwrap());
        assert_eq!(read("[]"), Some(Vec::new()));
        assert_eq!(read(r#"[{"id":"list.next"}]"#), Some(vec![told("list.next", "{}")]));
        assert_eq!(read(r#"[{"id":"list.next","params":null}]"#), Some(vec![told("list.next", "{}")]));
        assert_eq!(read(r#"[{"id":"mail.flag","params":{"on":true}}]"#), Some(vec![told("mail.flag", r#"{"on":true}"#)]));
        assert_eq!(read(r#"[{"id":"mail.flag","params":"{\"on\":true}"}]"#), Some(vec![told("mail.flag", r#"{"on":true}"#)]));
        let long = format!(r#"[{{"id":"{}"}}]"#, "x".repeat(MAX_ID + 1));
        let many = format!("[{}]", vec![r#"{"id":"list.next"}"#; MAX_TOLD + 1].join(","));
        for junk in [
            r#"{"id":"list.next"}"#,
            r#"["list.next"]"#,
            r#"[{"params":{}}]"#,
            r#"[{"id":7}]"#,
            r#"[{"id":""}]"#,
            "7",
            "null",
            long.as_str(),
            many.as_str(),
        ] {
            assert_eq!(read(junk), None, "{junk}");
        }
    }

    /// Each command's outcome is said, on one line; one that doesn't run stops the rest.
    #[test]
    fn the_window_says_how_each_command_went() {
        let mut a = app();
        // No account: nothing is listed, and a new message can't be started.
        let said = run(&mut a, &[told("list.messages", "{}"), told("mail.new", "{}"), told("list.messages", "{}")]);
        assert!(!said.contains('\n'), "one line");
        let said = ran(&said);
        assert_eq!(said.len(), 3);
        assert_eq!((&said[0]["id"], &said[0]["ok"]), (&json!("list.messages"), &json!(true)), "{said:?}");
        assert_eq!(said[1]["ok"], false, "{said:?}");
        assert!(said[1]["error"].as_str().is_some_and(|e| !e.is_empty()));
        assert_eq!(said[2], json!({"id": "list.messages", "ok": false, "error": "not run: the command before it wasn't"}));
        // Parameters that aren't JSON are said, and the command isn't run.
        let said = ran(&run(&mut a, &[told("list.messages", "{bad")]));
        assert!(said[0]["error"].as_str().unwrap().starts_with("bad JSON"), "{said:?}");
        // Nothing to run, nothing said to have run.
        assert_eq!(run(&mut a, &[]), r#"{"ran":[]}"#);
    }

    /// An outcome names its command and says whether it ran: with what it answered, if anything,
    /// or why it didn't.
    #[test]
    fn an_outcome_says_what_was_answered_or_why_not() {
        assert_eq!(outcome("x", Ok(Value::Null)), json!({"id": "x", "ok": true}));
        assert_eq!(outcome("x", Ok(json!({"n": 1}))), json!({"id": "x", "ok": true, "result": {"n": 1}}));
        assert_eq!(outcome("x", Err("why".into())), json!({"id": "x", "ok": false, "error": "why"}));
    }
}
