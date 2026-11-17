//! A reply drafted by a language model on this computer (`Job::DraftReply`).
//!
//! The model is run by llama.cpp's `llama-server`, started for the one draft on a port of this
//! computer alone (`127.0.0.1`) and stopped when the draft is written, or can't be: nothing is
//! left running, and the message never leaves the computer. Which program and which model file
//! are the user's choice; with none chosen, the ones TrueVoice (a dictation program) keeps are
//! used, so someone who has it needs nothing more.
//!
//! The model's answer is read as it's written (a stream of events), so a slow computer isn't
//! taken for one that stopped answering.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::{NetError, Result, http};

/// How long the model may take to load before the draft is given up (a few gigabytes read from
/// a slow disk).
const LOAD_WAIT: Duration = Duration::from_secs(180);
/// How often it's asked whether it has loaded.
const LOAD_POLL: Duration = Duration::from_millis(300);
/// The most the model writes in a draft (about 450 words), and the longest answer read.
const DRAFT_TOKENS: u32 = 600;
const ANSWER_MAX: usize = 1 << 20;
/// The model's working room, in tokens: the message, the instructions and the draft.
const CONTEXT: u32 = 8192;

/// Write a reply: `system` says how, `prompt` is the message to answer. `program` and `model`
/// are the paths the user chose, each empty for the one found ([`found_program`],
/// [`found_model`]).
pub fn draft(program: &str, model: &str, system: &str, prompt: &str) -> Result<String> {
    let program = chosen(program, found_program(), "the program that runs the model (llama-server)")?;
    let model = chosen(model, found_model(), "a model file (.gguf)")?;
    let port = free_port()?;
    let mut server = Server::start(&program, &model, port)?;
    server.wait_ready(port)?;
    let text = complete(port, system, prompt)?;
    drop(server);
    let text = without_thinking(&text);
    if text.is_empty() { Err(NetError::Protocol("the model wrote nothing".into())) } else { Ok(text) }
}

/// The path chosen, else the one found, as long as it's a file.
fn chosen(asked: &str, found: Option<PathBuf>, what: &str) -> Result<PathBuf> {
    let asked = asked.trim();
    let path = if asked.is_empty() { found } else { Some(PathBuf::from(asked)) };
    match path {
        Some(path) if path.is_file() => Ok(path),
        Some(path) => Err(NetError::Protocol(format!("{what} isn't at {}", path.display()))),
        None => Err(NetError::Protocol(format!("{what} wasn't found on this computer"))),
    }
}

/// Where TrueVoice keeps llama.cpp's server (Windows: under the user's local application data).
fn found_program() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let path = Path::new(&local).join("TrueVoice").join("engines").join("llama").join("llama-server.exe");
    path.is_file().then_some(path)
}

/// The model file TrueVoice keeps (Windows: under the user's application data): the first
/// `.gguf` by name, so it's the same one each time.
fn found_model() -> Option<PathBuf> {
    let roaming = std::env::var_os("APPDATA")?;
    first_model(&Path::new(&roaming).join("TrueVoice").join("models"))
}

/// The first `.gguf` file in `folder`, by name.
fn first_model(folder: &Path) -> Option<PathBuf> {
    let mut models: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")))
        .collect();
    models.sort();
    models.into_iter().next()
}

/// A port of this computer nothing listens on (asked of the system, then let go for the server).
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// The running `llama-server`, stopped when this is dropped.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Server {
    fn start(program: &Path, model: &Path, port: u16) -> Result<Server> {
        let mut command = Command::new(program);
        command
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-m")
            .arg(model)
            .arg("-c")
            .arg(CONTEXT.to_string())
            // All of it on the graphics card when there is one; one draft at a time.
            .args(["-ngl", "99", "-np", "1", "--no-warmup"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Its libraries are found beside it.
        if let Some(folder) = program.parent().filter(|f| !f.as_os_str().is_empty()) {
            command.current_dir(folder);
        }
        #[cfg(windows)]
        {
            // No console window flashes up (CREATE_NO_WINDOW).
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn().map_err(|e| NetError::Protocol(format!("{} couldn't be started: {e}", program.display())))?;
        Ok(Server(child))
    }

    /// Wait until the model has loaded: its `/health` answers 200 then, 503 before.
    fn wait_ready(&mut self, port: u16) -> Result<()> {
        let until = Instant::now() + LOAD_WAIT;
        loop {
            if let Some(status) = self.0.try_wait()? {
                return Err(NetError::Protocol(format!("the model's program stopped before it was ready ({status})")));
            }
            if http::get(&format!("http://127.0.0.1:{port}/health"), 1 << 16).is_ok_and(|r| r.ok()) {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(NetError::Protocol("the model took too long to load".into()));
            }
            std::thread::sleep(LOAD_POLL);
        }
    }
}

/// What's asked of the server: one chat completion, written as a stream.
fn request_body(system: &str, prompt: &str) -> Vec<u8> {
    json!({
        "messages": [{"role": "system", "content": system}, {"role": "user", "content": prompt}],
        "temperature": 0.3,
        "max_tokens": DRAFT_TOKENS,
        "stream": true,
    })
    .to_string()
    .into_bytes()
}

/// Ask the server on `port` for the completion, and read what it wrote.
fn complete(port: u16, system: &str, prompt: &str) -> Result<String> {
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let answer = http::send(&http::Request::post(&url, "application/json", request_body(system, prompt), ANSWER_MAX))?;
    let body = String::from_utf8_lossy(&answer.body);
    if !answer.ok() {
        let said: String = body.chars().take(200).collect();
        return Err(NetError::Protocol(format!("the model's program answered {}: {}", answer.status, said.trim())));
    }
    Ok(read_stream(&body))
}

/// The text of a streamed chat completion: each `data:` line's piece (`choices[0].delta.content`),
/// one after another, up to `[DONE]`. An answer that wasn't streamed (one JSON object, its text in
/// `choices[0].message.content`) is read too.
fn read_stream(body: &str) -> String {
    let piece =
        |event: &Value, part: &str| -> Option<String> { event.get("choices")?.get(0)?.get(part)?.get("content")?.as_str().map(str::to_string) };
    let mut text = String::new();
    let mut streamed = false;
    for line in body.lines() {
        let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
        streamed = true;
        if data == "[DONE]" {
            break;
        }
        if let Ok(event) = serde_json::from_str::<Value>(data)
            && let Some(more) = piece(&event, "delta")
        {
            text.push_str(&more);
        }
    }
    if !streamed
        && let Ok(whole) = serde_json::from_str::<Value>(body.trim())
        && let Some(all) = piece(&whole, "message")
    {
        text = all;
    }
    text
}

/// The draft without a reasoning model's thoughts (`<think>…</think>`, which some write before
/// the answer), and without space around it.
fn without_thinking(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(rest.get(..start).unwrap_or_default());
        rest = match rest.get(start..).and_then(|r| r.find("</think>").map(|end| (r, end))) {
            Some((r, end)) => r.get(end + "</think>".len()..).unwrap_or_default(),
            // Thoughts never closed: nothing after them is an answer.
            None => "",
        };
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn a_streamed_answer_is_put_together() {
        let body = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":null}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"Hi Peter,\\n\\n\"}}]}\n\n\
                    : a comment\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"Thanks, but no.\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                    data: [DONE]\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\" (after the end)\"}}]}\n";
        assert_eq!(read_stream(body), "Hi Peter,\n\nThanks, but no.");
        // Not streamed after all.
        assert_eq!(read_stream("{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Hello.\"}}]}"), "Hello.");
        // Junk is nothing, never a panic.
        for junk in ["", "data:", "data: {", "data: 7\ndata: []\ndata: {\"choices\":7}", "{\"choices\":[]}", "<html>busy</html>"] {
            assert_eq!(read_stream(junk), "", "{junk:?}");
        }
    }

    #[test]
    fn thoughts_are_left_out_of_the_draft() {
        assert_eq!(without_thinking("  Hi Sam,\nYes.\n"), "Hi Sam,\nYes.");
        assert_eq!(without_thinking("<think>They ask about Thursday.</think>\n\nHi Sam,\nYes."), "Hi Sam,\nYes.");
        assert_eq!(without_thinking("Hi <think>a</think>Sam<think>b</think>."), "Hi Sam.");
        assert_eq!(without_thinking("Hi Sam.<think>never closed"), "Hi Sam.");
        assert_eq!(without_thinking("<think>only thoughts</think>"), "");
    }

    #[test]
    fn the_request_asks_for_one_streamed_completion() {
        let asked: Value = serde_json::from_slice(&request_body("Be brief.", "From: a\n\nHello")).unwrap();
        assert_eq!(asked["messages"][0], json!({"role": "system", "content": "Be brief."}));
        assert_eq!(asked["messages"][1], json!({"role": "user", "content": "From: a\n\nHello"}));
        assert_eq!(asked["stream"], true);
        assert_eq!(asked["max_tokens"], DRAFT_TOKENS);
    }

    #[test]
    fn a_program_or_model_that_isn_t_there_is_said() {
        let folder = std::env::temp_dir().join(format!("mailcraft-assistant-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let (b, a, other) = (folder.join("b.gguf"), folder.join("A.GGUF"), folder.join("notes.txt"));
        for file in [&b, &a, &other] {
            std::fs::write(file, b"x").unwrap();
        }
        // The first model by name, whatever else is in the folder; none in one without any.
        assert_eq!(first_model(&folder), Some(a.clone()));
        assert_eq!(first_model(&folder.join("none")), None);
        assert_eq!(chosen("", Some(a.clone()), "a model").unwrap(), a);
        assert_eq!(chosen(b.to_str().unwrap(), Some(a.clone()), "a model").unwrap(), b);
        let missing = chosen(folder.join("gone.gguf").to_str().unwrap(), Some(a.clone()), "a model file").unwrap_err().to_string();
        assert!(missing.contains("a model file isn't at"), "{missing}");
        let none = chosen("  ", None, "a model file").unwrap_err().to_string();
        assert!(none.contains("wasn't found on this computer"), "{none}");
        // A draft with nothing to run says so, and starts nothing.
        let said = draft(folder.join("no-server").to_str().unwrap(), a.to_str().unwrap(), "s", "p").unwrap_err().to_string();
        assert!(said.contains("llama-server"), "{said}");
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// A stand-in server on this computer: `complete` posts the request and reads the stream;
    /// an answer that isn't 200 is an error with what the server said.
    #[test]
    fn the_completion_is_asked_of_the_server_on_this_computer() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut asked = Vec::new();
            for answer in [
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hi \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"there.\"}}]}\n\ndata: [DONE]\n\n",
                "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":{\"message\":\"Loading model\"}}",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                // The head, then as much body as it says.
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&request).to_string();
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let length = text
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                            .unwrap_or(0);
                        if request.len() >= head_end + 4 + length || n == 0 {
                            break;
                        }
                    }
                }
                asked.push(String::from_utf8_lossy(&request).to_string());
                stream.write_all(answer.as_bytes()).unwrap();
            }
            asked
        });
        assert_eq!(complete(port, "Be brief.", "Hello").unwrap(), "Hi there.");
        let refused = complete(port, "Be brief.", "Hello").unwrap_err().to_string();
        assert!(refused.contains("503") && refused.contains("Loading model"), "{refused}");
        let asked = server.join().unwrap();
        assert!(asked[0].starts_with("POST /v1/chat/completions HTTP/1.1"), "{}", asked[0]);
        assert!(asked[0].contains("\"stream\":true") && asked[0].contains("Be brief."), "{}", asked[0]);
    }
}
