//! One local process and one fresh ACP v2 Preview session per generation.
use async_process::{ChildStdin, Command};
use futures_lite::{
    future,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};

async fn send(input: &mut ChildStdin, message: Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    input
        .write_all(&bytes)
        .await
        .map_err(|e| format!("ACP stdin: {e}"))
}
fn request(id: u8, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}

// Position belongs to the first update, not the most recent chunk. In
// particular, a correction to an earlier message isn't the final answer.
#[derive(Default)]
struct Messages(Vec<(String, String)>);
impl Messages {
    fn update(&mut self, u: &Value, chunk: bool) -> Result<(), String> {
        let id = u["messageId"]
            .as_str()
            .ok_or("ACP agent message lacks messageId")?;
        let i = match self.0.iter().position(|(k, _)| k == id) {
            Some(i) => i,
            None if self.0.len() < 256 => {
                self.0.push((id.into(), String::new()));
                self.0.len() - 1
            }
            None => return Err("Too many ACP agent messages".into()),
        };
        let text = &mut self.0[i].1;
        let blocks = if chunk {
            std::slice::from_ref(u.get("content").ok_or("Missing ACP chunk content")?)
        } else {
            match u.get("content") {
                None => return Ok(()),
                Some(v) => {
                    text.clear();
                    if v.is_null() {
                        return Ok(());
                    }
                    v.as_array()
                        .ok_or("Invalid ACP message content")?
                        .as_slice()
                }
            }
        };
        for b in blocks {
            if b["type"] == "text" {
                let s = b["text"].as_str().ok_or("Malformed ACP text block")?;
                if text.len() + s.len() > 128 * 1024 {
                    return Err("Oversized ACP walkthrough".into());
                }
                text.push_str(s);
            }
        }
        Ok(())
    }
}

pub(super) async fn walkthrough(
    mut cmd: Command,
    bundle: &Path,
    source: Option<&Path>,
    prompt: &str,
    mut report: impl FnMut(usize),
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<String, String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Cannot start ACP agent: {e}"))?;
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut session = None::<String>;
    let exchange = async {
        let mut pending = Some(1);
        send(&mut input,request(1,"initialize",json!({"protocolVersion":2,"info":{"name":"ferriscope","version":env!("CARGO_PKG_VERSION")},"capabilities":{}}))).await?;
        let mut messages = Messages::default();
        let mut events = 0;
        let mut complete = false;
        let mut active = false;
        loop {
            let mut line = Vec::new();
            (&mut output)
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|e| format!("ACP stdout: {e}"))?;
            if line.is_empty() {
                return Err("ACP agent disconnected before completing the walkthrough. Check local authentication and agent configuration.".into());
            }
            if line.len() > 1024 * 1024 {
                return Err("Oversized ACP stdio frame".into());
            }
            let m: Value =
                serde_json::from_slice(&line).map_err(|e| format!("Malformed ACP JSON: {e}"))?;
            if m["jsonrpc"] != "2.0" {
                return Err("Invalid ACP JSON-RPC envelope".into());
            }
            if let Some(method) = m["method"].as_str() {
                if let Some(id) = m.get("id") {
                    let response = if method == "session/request_permission" {
                        json!({"jsonrpc":"2.0","id":id,"result":{"outcome":{"outcome":"cancelled"}}})
                    } else {
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unsupported client method"}})
                    };
                    send(&mut input, response).await?;
                } else if method == "session/update"
                    && session.is_some()
                    && m["params"]["sessionId"].as_str() == session.as_deref()
                {
                    let u = &m["params"]["update"];
                    match u["sessionUpdate"]
                        .as_str()
                        .ok_or("Malformed ACP session update")?
                    {
                        "agent_message_chunk" => messages.update(u, true)?,
                        "agent_message" => messages.update(u, false)?,
                        "state_update"
                            if u["state"] == "running" || u["state"] == "requires_action" =>
                        {
                            active = true
                        }
                        "state_update" if (active || pending.is_none()) && u["state"] == "idle" => {
                            if u["stopReason"] != "end_turn" {
                                return Err(format!(
                                    "ACP agent stopped: {}",
                                    u["stopReason"].as_str().unwrap_or("no stop reason")
                                ));
                            }
                            complete = true;
                        }
                        _ => {}
                    }
                    events += 1;
                    report(events);
                }
            } else {
                let id = pending
                    .take()
                    .ok_or("Unexpected ACP response after prompt acceptance")?;
                if m["id"].as_u64() != Some(id) {
                    return Err("Mismatched ACP response ID".into());
                }
                if let Some(e) = m.get("error") {
                    if e["code"] == -32022 {
                        return Err(
                            "Incompatible ACP version: Ferriscope requires v2 Preview.".into()
                        );
                    }
                    return Err(format!(
                        "ACP request failed: {}",
                        e["message"].as_str().unwrap_or("unknown error")
                    ));
                }
                let r = &m["result"];
                match id {
                    1 => {
                        if r["protocolVersion"] != 2 {
                            return Err(format!(
                                "Incompatible ACP protocol version {} (requires v2 Preview)",
                                r["protocolVersion"]
                            ));
                        }
                        if !r["capabilities"]["session"].is_object() {
                            return Err("ACP agent does not advertise session support".into());
                        }
                        if !r["info"]["name"].is_string() || !r["info"]["version"].is_string() {
                            return Err("Missing ACP v2 agent information".into());
                        }
                        let mut params = json!({"cwd":bundle});
                        if let Some(source) = source
                            && r["capabilities"]["session"]["additionalDirectories"].is_object()
                        {
                            params["additionalDirectories"] = json!([source]);
                        }
                        send(&mut input, request(2, "session/new", params)).await?;
                        pending = Some(2);
                    }
                    2 => {
                        session = Some(
                            r["sessionId"]
                                .as_str()
                                .ok_or("Missing ACP sessionId")?
                                .into(),
                        );
                        send(&mut input,request(3,"session/prompt",json!({"sessionId":session,"prompt":[{"type":"text","text":prompt}]}))).await?;
                        pending = Some(3);
                    }
                    _ => {
                        r["messageId"]
                            .as_str()
                            .ok_or("Invalid ACP prompt acknowledgment")?;
                    }
                }
            }
            if complete && pending.is_none() {
                return messages
                    .0
                    .pop()
                    .map(|(_, s)| s)
                    .ok_or_else(|| "ACP agent finished without a walkthrough message".into());
            }
        }
    };
    let outcome = future::race(
        async { Some(tokio::time::timeout(Duration::from_secs(600), exchange).await) },
        async {
            let _ = cancel.await;
            None
        },
    )
    .await;
    let result = match outcome {
        Some(Ok(result)) => result,
        Some(Err(_)) => Err("ACP walkthrough timed out.".into()),
        None => {
            if let Some(id) = session {
                let _=tokio::time::timeout(Duration::from_millis(500),send(&mut input,json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":id}}))).await;
            }
            Err("Generation cancelled.".into())
        }
    };
    // EOF is the stdio disconnect signal. Give the adapter time to stop its
    // session and subprocesses; kill_on_drop also covers component unmount.
    drop(input);
    let _ = tokio::time::timeout(Duration::from_millis(750), child.status()).await;
    result
}
