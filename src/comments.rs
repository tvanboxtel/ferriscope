use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub path: String,
    pub side: String,
    pub line: u32,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[cfg(not(target_arch = "wasm32"))]
pub struct Note {
    #[serde(flatten)]
    pub anchor: Anchor,
    pub body: String,
}

/// Unified hunks have independent old and new coordinates. Context is anchored
/// on the new side; metadata (including no-newline markers) consumes no lines.
pub fn coordinates(patch: &str) -> impl Iterator<Item = Option<(&'static str, u32)>> + '_ {
    let mut cursor = None;
    patch.lines().map(move |line| {
        if let Some(header) = line.strip_prefix("@@ -") {
            cursor = (|| {
                let (old, rest) = header.split_once(" +")?;
                let (new, _) = rest.split_once(" @@")?;
                let start = |s: &str| s.split(',').next()?.parse::<u32>().ok();
                Some((start(old)?, start(new)?))
            })();
            return None;
        }
        let (old, new) = cursor.as_mut()?;
        match line.as_bytes().first()? {
            b'-' => {
                let n = *old;
                *old += 1;
                Some(("LEFT", n))
            }
            b'+' => {
                let n = *new;
                *new += 1;
                Some(("RIGHT", n))
            }
            b' ' => {
                let n = *new;
                *old += 1;
                *new += 1;
                Some(("RIGHT", n))
            }
            _ => None,
        }
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop {
    use super::*;
    use crate::platform::store;
    use dioxus::prelude::*;
    use futures_lite::io::AsyncWriteExt;
    use std::process::Stdio;

    #[derive(Clone, Default, Serialize, Deserialize)]
    struct Draft {
        head: String,
        notes: Vec<Note>,
    }

    async fn publish(pr: &str, draft: &Draft) -> Result<(), String> {
        publish_with(std::ffi::OsStr::new("gh"), pr, draft).await
    }
    async fn publish_with(gh: &std::ffi::OsStr, pr: &str, draft: &Draft) -> Result<(), String> {
        let api = format!("repos/{}", pr.replace("/pull/", "/pulls/"));
        let head = async_process::Command::new(gh)
            .args(["api", "--hostname", "github.com", &api, "--jq", ".head.sha"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if !head.status.success() {
            return Err(String::from_utf8_lossy(&head.stderr).into_owned());
        }
        if String::from_utf8_lossy(&head.stdout).trim() != draft.head {
            return Err(
                "The PR changed. Refresh and re-anchor your notes before submitting.".into(),
            );
        }
        let body =
            serde_json::json!({"event":"COMMENT","body":"Inline review comments","commit_id":draft.head,"comments":draft.notes})
                .to_string();
        let mut child = async_process::Command::new(gh)
            .args([
                "api",
                "--hostname",
                "github.com",
                "--method",
                "POST",
                &format!("{api}/reviews"),
                "--input",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let result = child.output().await.map_err(|e| e.to_string())?;
        if result.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&result.stderr).into_owned())
        }
    }

    #[cfg(all(test, unix))]
    #[test]
    fn fake_gh_checks_head_then_posts_one_review() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let gh = dir.path().join("gh");
        fs::write(&gh,r#"#!/bin/sh
case "$*" in
  'api --hostname github.com repos/o/r/pulls/1 --jq .head.sha') echo current ;;
  'api --hostname github.com --method POST repos/o/r/pulls/1/reviews --input -') cat > "$0.payload" ;;
  *) exit 1 ;;
esac"#).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o700)).unwrap();
        let mut draft = Draft {
            head: "old".into(),
            notes: vec![Note {
                anchor: Anchor {
                    path: "src/a.rs".into(),
                    side: "LEFT".into(),
                    line: 4,
                },
                body: "Why remove this?".into(),
            }],
        };
        let run = |d: &Draft| {
            futures_lite::future::block_on(publish_with(gh.as_os_str(), "o/r/pull/1", d))
        };
        let payload = gh.with_extension("payload");
        assert!(run(&draft).is_err());
        assert!(!payload.exists());
        draft.head = "current".into();
        run(&draft).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&fs::read(payload).unwrap()).unwrap();
        assert_eq!(json["commit_id"], "current");
        assert_eq!(json["event"], "COMMENT");
        assert_eq!(
            json["comments"][0],
            serde_json::json!({"path":"src/a.rs","side":"LEFT","line":4,"body":"Why remove this?"})
        );
    }
    #[test]
    fn maps_hunks_and_no_newline_marker() {
        assert_eq!(coordinates("@@ -4,2 +8,2 @@ fn a\n-old\n\\ No newline at end of file\n+new\n same\n@@ -0,0 +15 @@\n+more").collect::<Vec<_>>(),[None,Some(("LEFT",4)),None,Some(("RIGHT",8)),Some(("RIGHT",9)),None,Some(("RIGHT",15))]);
    }
    #[component]
    pub fn Tray(pr: String, head: String, mut anchor: Signal<Option<Anchor>>) -> Element {
        let key = use_hook(|| format!("draft:{pr}"));
        let loaded = use_hook(|| {
            store::read(&key)
                .and_then(|v| {
                    v.map(|s| serde_json::from_str::<Draft>(&s))
                        .transpose()
                        .map_err(|e| e.to_string())
                })
                .map(Option::unwrap_or_default)
        });
        let mut draft = use_signal(|| loaded.clone().unwrap_or_default());
        let mut message = use_signal(|| loaded.clone().err().unwrap_or_default());
        let mut busy = use_signal(|| false);
        let mut text = use_signal(String::new);
        let stale = !draft.read().notes.is_empty() && draft.read().head != head;
        let persist = use_callback(move |next: Draft| {
            let json = serde_json::to_string(&next).unwrap();
            match store::write(&key, &json) {
                Ok(()) => {
                    draft.set(next);
                    message.set(String::new());
                    true
                }
                Err(e) => {
                    message.set(e);
                    false
                }
            }
        });
        let target = anchor();
        let notes = draft.read().notes.clone();
        rsx! {
            if !message.read().is_empty(){p {role:"status","{message}"}}
            if target.is_some() || !notes.is_empty(){
                section {class:"drafts",onkeydown:move|e|e.stop_propagation(),
                    strong {"Draft review · {notes.len()} saved"}
                    if stale {p {"These notes refer to an older revision. Copy their text, remove them, then select lines in the new diff."}}
                    for (i,n) in notes.iter().enumerate(){div {class:"draft-note",
                        b {"{n.anchor.path} · {n.anchor.side} {n.anchor.line}"} p {"{n.body}"}
                        button {disabled:busy(),onclick:move |_|{let mut d=draft();d.notes.remove(i);persist.call(d);},"Remove"}
                    }}
                    if let Some(a)=target {
                        label {"Comment on {a.path} · {a.side} {a.line}"
                            textarea {value:"{text}",onmounted:move |e|async move{let _=e.set_focus(true).await;}, oninput:move |e|text.set(e.value())}
                        }
                        button {disabled:busy()||stale||text.read().trim().is_empty()||loaded.is_err(),onclick:{
                            let head=head.clone();move |_|{let mut d=draft();d.head=head.clone();d.notes.push(Note{anchor:a.clone(),body:text()});if persist.call(d){text.set(String::new());anchor.set(None)}}
                        },"Save draft"}
                        button {onclick:move |_|{text.set(String::new());anchor.set(None)},"Cancel"}
                    }
                    if !notes.is_empty(){
                        button {disabled:busy()||stale||anchor.read().is_some(),onclick:move |_|{
                            if busy(){return} busy.set(true);let pr=pr.clone();let d=draft();
                            spawn(async move{match publish(&pr,&d).await{
                                Ok(())=>{if persist.call(Draft::default()){message.set("Review posted to GitHub.".into())}else{draft.set(Draft::default());message.set("Review posted, but the local draft could not be cleared. Do not resubmit it.".into())}},
                                Err(e)=>message.set(e)
                            }busy.set(false);});
                        },"Submit {notes.len()} inline comments to GitHub"}
                    }
                }
            }
        }
    }
}
