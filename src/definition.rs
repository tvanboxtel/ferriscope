//! Reader-scoped PR checkout and TypeScript session.
use dioxus::prelude::*;
use futures_lite::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use futures_util::{
    FutureExt,
    future::{LocalBoxFuture, Shared},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::rc::Rc;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use url::Url;

#[derive(Clone, Deserialize)]
struct Selection {
    line: usize, // GitHub new-file coordinate, one-based
    before: String,
    symbol: String,
    after: String,
}

// Hit-test the actual glyph, not the current selection or a visual column.
// Build the offset across all the syntax spans in this source line.
const HIT_TEST: &str = r#"
const el = document.elementFromPoint(x,y)?.closest('[data-definition-line]');
if (!el || !el.closest('.definition-diff')) return null;
const pos = document.caretPositionFromPoint?.(x,y);
const caret = pos ? null : document.caretRangeFromPoint?.(x,y);
const node = pos?.offsetNode ?? caret?.startContainer;
const offset = pos?.offset ?? caret?.startOffset;
if (!node || node.nodeType !== Node.TEXT_NODE || !el.contains(node)) return null;
const range = document.createRange();
const text = el.textContent;
for (const i of [offset, offset-1]) {
    if (i < 0 || i >= node.length) continue;
    range.setStart(node,i); range.setEnd(node,i+1);
    const box = range.getBoundingClientRect();
    if (x < box.left || x >= box.right || y < box.top || y >= box.bottom) continue;
    range.setStart(el,0); range.setEnd(node,i);
    const index = range.toString().length;
    for (const m of text.matchAll(/[$_\p{ID_Start}][$\u200c\u200d\p{ID_Continue}]*/gu)) {
        if (m.index > index) break;
        if (index < m.index+m[0].length) return {line:Number(el.dataset.definitionLine),
            before:text.slice(0,m.index),symbol:m[0],after:text.slice(m.index+m[0].length)};
    }
}
return null;
"#;

#[derive(Clone)]
struct Source {
    path: String,
    text: String,
    line: usize,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Pair {
    root: String,
    server: String,
    #[serde(default)]
    allow_fetch: bool,
    #[serde(default)]
    warm_index: bool,
}
fn detect_server() -> String {
    let mut dirs: Vec<_> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|dir| dir.is_absolute())
        .collect();
    #[cfg(target_os = "macos")]
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(data) = dirs::data_local_dir() {
        dirs.push(data.join("zed/languages/vtsls/node_modules/.bin"));
    }
    for dir in dirs {
        for name in ["typescript-language-server", "vtsls"] {
            let path = dir.join(name);
            if path.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if !path
                        .metadata()
                        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
                    {
                        continue;
                    }
                }
                return path.to_string_lossy().into_owned();
            }
        }
    }
    String::new()
}

#[derive(Clone, Copy)]
struct SettingsRevision(Signal<u64>);
pub(crate) fn use_preferences() {
    let revision = use_signal(|| 0);
    use_context_provider(|| SettingsRevision(revision));
}
fn saved(repo: &str) -> Option<Pair> {
    crate::platform::store::read(&format!("definition-trust:{repo}"))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
}
#[component]
pub fn NavigableDiff(
    path: String,
    head: String,
    mut reading: Signal<bool>,
    #[props(default)] on_follow: Option<Callback<()>>,
    children: Element,
) -> Element {
    let session = use_context::<Memo<Option<Navigation>>>();
    let mut source = use_context::<Signal<Option<Source>>>();
    let mut busy = use_signal(|| false);
    let mut status = use_context::<LookupMessage>().0;
    use_drop(move || status.set(None));
    let run = use_callback({
        let (path, head) = (path.clone(), head.clone());
        move |s: Selection| {
            if busy() {
                return;
            }
            let Some(nav) = session() else { return };
            let (path, head) = (path.clone(), head.clone());
            busy.set(true);
            status.set(Some((true, format!("Finding definition of {}…", s.symbol))));
            spawn(async move {
                match nav.0.prepared.clone().await {
                    Ok(Some(dir)) => {
                        match lookup(&nav.0.client, dir.path(), &nav.0.server, &head, &path, &s)
                            .await
                        {
                            Ok(view) => {
                                if let Some(follow) = on_follow {
                                    follow.call(());
                                }
                                source.set(Some(view));
                                reading.set(true);
                                status.set(None);
                            }
                            Err(e) => status.set(Some((false, e))),
                        }
                    }
                    Ok(None) => {
                        status.set(Some((false,"Commit not available locally. Enable “Fetch missing PR commits” in Code navigation.".into())));
                    }
                    Err(e) => status.set(Some((false, e))),
                }
                busy.set(false);
            });
        }
    });
    let gesture = if cfg!(target_os = "macos") {
        "⌘-click"
    } else {
        "Ctrl-click"
    };
    rsx! {
        div {class:"definition-diff",title:session.read().as_ref().map(|_|format!("{gesture} an identifier to go to its definition")),
            onclick:move |e:MouseEvent|{
                let modifiers=e.modifiers();
                let modified=modifiers==Modifiers::CONTROL || (cfg!(target_os="macos") && modifiers==Modifiers::META);
                let point=e.client_coordinates();
                async move{
                    if !modified || busy() || e.trigger_button()!=Some(dioxus::html::input_data::MouseButton::Primary){return}
                    if session.read().is_none(){return}
                    let js=format!("const x={}, y={};{HIT_TEST}",point.x,point.y);
                    let Ok(Some(s))=document::eval(&js).join::<Option<Selection>>().await else{return};
                    run.call(s);
                }
            },{children}
        }
    }
}

#[component]
pub fn SourceSettings(initial_repo: String) -> Element {
    let mut revision = use_context::<SettingsRevision>().0;
    let mut root = use_signal(String::new);
    let mut repo = use_signal(|| initial_repo);
    let mut server = use_signal(detect_server);
    let mut allow_fetch = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let existing = use_memo(move || {
        revision.read();
        saved(&repo.read().trim().to_ascii_lowercase())
    });
    rsx! {
        details {class:"definition-onboarding",
            summary {"Code navigation"}
            div {class:"definition-setup",
                strong {"TypeScript definitions"}
                if existing().is_none(){
                p {"Connect a local checkout once to follow symbols from PR diffs."}
                button {disabled:busy(),onclick:move |_|async move{
                    if let Some(folder)=rfd::AsyncFileDialog::new().set_title("Choose the local repository root").pick_folder().await{
                        root.set(folder.path().to_string_lossy().into_owned());
                        let remote=git(folder.path(),&["remote","get-url","origin"]).await.unwrap_or_default();
                        repo.set(github_repo(String::from_utf8_lossy(&remote).trim()).unwrap_or_default());
                        allow_fetch.set(false);
                        message.set(String::new());
                    }
                },"Choose local repository…"}
                label {"Checkout folder" input {value:"{root}",placeholder:"Or enter an absolute path",disabled:busy(),oninput:move |e|root.set(e.value())}}
                details {
                    summary {if server.read().is_empty() {"Choose a TypeScript server"}else{"TypeScript server: {server}"}}
                    label {"Executable path" input {value:"{server}",placeholder:"Absolute executable path",disabled:busy(),oninput:move |e|server.set(e.value())}}
                }
                span {"Trust only a checkout, plugins and executable you know. Connecting does not switch branches or start a server."}
                }
                label {"GitHub repository" input {value:"{repo}",placeholder:"owner/repository",disabled:busy(),oninput:move |e|{repo.set(e.value());allow_fetch.set(false);message.set(String::new())}}}
                if let Some(pair)=existing(){span {class:"connected-checkout","Connected · {pair.root}"}}
                label {class:"fetch-option",input {r#type:"checkbox",disabled:busy()||repo.read().trim().is_empty(),
                    checked:existing().map_or_else(||allow_fetch(),|p|p.allow_fetch),
                    onchange:move |e|{
                        if let Some(mut pair)=existing(){
                            pair.allow_fetch=e.checked();
                            let name=repo().trim().to_ascii_lowercase();
                            match save_pair(&name,&pair){
                                Ok(())=>{revision+=1;message.set(format!("Fetch preference saved for {name}."));}
                                Err(e)=>message.set(e),
                            }
                        }else{allow_fetch.set(e.checked())}
                    }
                } "Fetch missing PR commits automatically"}
                if existing().is_none(){
                button {disabled:busy()||repo.read().trim().is_empty()||root.read().is_empty()||server.read().is_empty(),onclick:move |_|async move{
                    busy.set(true);
                    let name=repo().trim().to_ascii_lowercase();
                    let result=connect(&name,&root(),&server(),existing().map_or_else(||allow_fetch(),|p|p.allow_fetch)).await;
                    message.set(match result {Ok(())=>{revision+=1;format!("Connected {name}. Ctrl-click a TypeScript identifier to look up its definition.")},Err(e)=>e});
                    busy.set(false);
                },"Trust and connect"}
                }else{
                    label {class:"fetch-option",input {r#type:"checkbox",checked:existing().is_some_and(|p|p.warm_index),
                        onchange:move |e|if let Some(mut pair)=existing(){
                            pair.warm_index=e.checked();
                            let name=repo().trim().to_ascii_lowercase();
                            match save_pair(&name,&pair){
                                Ok(())=>{revision+=1;message.set(format!("Indexing preference saved for {name}."));}
                                Err(e)=>message.set(e)
                            }
                        }
                    } "Start TypeScript indexing when opening a PR"}
                    button {disabled:busy(),onclick:move |_|{
                        let key=format!("definition-trust:{}",repo.read().trim().to_ascii_lowercase());
                        match crate::platform::store::write(&key,"null"){Ok(())=>{allow_fetch.set(false);revision+=1;message.set("Disconnected.".into());},Err(e)=>message.set(e)}
                    },"Disconnect checkout"}
                }
                span {role:"status","{message}"}
            }
        }
    }
}
fn github_repo(remote: &str) -> Option<String> {
    let slug = [
        "git@github.com:",
        "https://github.com/",
        "ssh://git@github.com/",
    ]
    .iter()
    .find_map(|p| remote.strip_prefix(p))?
    .trim_end_matches('/');
    Some(
        slug.strip_suffix(".git")
            .unwrap_or(slug)
            .to_ascii_lowercase(),
    )
}
async fn connect(repo: &str, root: &str, server: &str, allow_fetch: bool) -> ResultOf<()> {
    let expected = format!("{repo}/pull/1");
    if ferriscope::pr_key(&format!("https://github.com/{expected}"))
        .ok()
        .as_deref()
        != Some(expected.as_str())
    {
        return Err("Enter a GitHub repository as owner/repository.".into());
    }
    if !Path::new(root).is_absolute() || !Path::new(server).is_absolute() {
        return Err("Use absolute paths for the checkout and server.".into());
    }
    let dir = Path::new(root)
        .canonicalize()
        .map_err(|_| "Checkout folder not found.")?;
    if git(&dir, &["rev-parse", "--is-inside-work-tree"])
        .await?
        .trim_ascii()
        != b"true"
        || !git(&dir, &["rev-parse", "--show-prefix"])
            .await?
            .trim_ascii()
            .is_empty()
    {
        return Err("Choose the root of a Git checkout.".into());
    }
    let exe = Path::new(server)
        .canonicalize()
        .map_err(|_| "Server executable not found.")?;
    if !exe.is_file() {
        return Err("Choose a server executable, not a directory.".into());
    }
    let pair = Pair {
        warm_index: false,
        allow_fetch,
        root: dir.to_string_lossy().into_owned(),
        server: exe.to_string_lossy().into_owned(),
    };
    save_pair(repo, &pair)
}
fn save_pair(repo: &str, pair: &Pair) -> ResultOf<()> {
    crate::platform::store::write(
        &format!("definition-trust:{repo}"),
        &serde_json::to_string(&pair).map_err(|e| e.to_string())?,
    )
}

pub(crate) fn supports(path: &str) -> bool {
    matches!(path.rsplit('.').next(), Some("ts" | "tsx" | "mts" | "cts"))
}
pub(crate) fn use_session(pr: &str, head: &str, first_ts: Option<&str>) {
    let (pr, head) = (pr.to_owned(), head.to_owned());
    let first_ts = first_ts.map(str::to_owned);
    let typescript = first_ts.is_some();
    let phase = use_signal(Vec::<PhaseTiming>::new);
    let changed = use_context::<SettingsRevision>().0;
    let config = use_memo(move || {
        changed.read();
        typescript
            .then(|| saved(pr_repo(&pr)))
            .flatten()
            .map(|pair| (pair, pr.clone(), head.clone()))
    });
    let session = use_memo(move || {
        config().map(|(pair, pr, head)| {
            let warm = pair
                .warm_index
                .then(|| (first_ts.clone().unwrap(), head.clone()));
            let prepared = async move {
                tokio::time::timeout(
                    Duration::from_secs(120),
                    prepare(&pair.root, &head, &pr, pair.allow_fetch, phase),
                )
                .await
                .map_err(|_| {
                    "Preparing PR source timed out. Refresh the PR to retry.".to_string()
                })?
            }
            .boxed_local()
            .shared();
            Navigation(Rc::new(Workspace {
                prepared,
                server: pair.server,
                warm,
                client: Default::default(),
            }))
        })
    });
    use_context_provider(|| session);
    let preparation = use_resource(move || {
        let nav = session();
        async move {
            if let Some(nav) = nav {
                if let Ok(Some(dir)) = nav.0.prepared.clone().await
                    && let Some((path, head)) = &nav.0.warm
                {
                    // Share the same lock as clicks. A click during startup waits
                    // for this server instead of launching a competing tsserver.
                    let _ = timed_phase(
                        phase,
                        "Starting TypeScript server",
                        tokio::time::timeout(Duration::from_secs(25), async {
                            let mut slot = nav.0.client.lock().await;
                            if slot.is_none() {
                                let file = dir.path().join(path);
                                let (_, text) = tracked_text(dir.path(), &file, head).await?;
                                let mut server = Server::start(dir.path(), &nav.0.server).await?;
                                server.open(&file, path, text).await?;
                                *slot = Some(server);
                            }
                            Ok::<_, String>(())
                        }),
                    )
                    .await;
                }
            }
        }
    });
    use_context_provider(|| PrepStatus(preparation, phase));
    let status = use_signal(|| None);
    use_context_provider(|| LookupMessage(status));
    let view = use_signal(|| None::<Source>);
    use_context_provider(|| view);
}
fn pr_repo(pr: &str) -> &str {
    pr.split_once("/pull/").expect("validated PR key").0
}
#[derive(Clone, Copy)]
struct PrepStatus(Resource<()>, Signal<Vec<PhaseTiming>>);

#[derive(Clone)]
struct PhaseTiming {
    name: &'static str,
    started: std::time::Instant,
    elapsed: Option<Duration>,
}

#[component]
pub fn LookupStatus() -> Element {
    let status = use_context::<LookupMessage>().0;
    let PrepStatus(prep, phase) = use_context::<PrepStatus>();
    let session = use_context::<Memo<Option<Navigation>>>();
    let timings = phase.read();
    let name = timings.last().map_or("Preparing PR source", |p| p.name);
    rsx! {if session.read().is_some() && prep.state()()==UseResourceState::Pending {
        span {class:"lookup-status",role:"status",b {class:"lookup-spinner",aria_hidden:"true"} "{name}…"}
    }else if let Some((busy,text))=status.read().as_ref(){
        span {class:"lookup-status",role:"status",if *busy{b {class:"lookup-spinner",aria_hidden:"true"}} "{text}"}
    }
    if session.read().is_some() && !timings.is_empty(){
        details {class:"source-timings",summary {"Source timings"}
            ul {for timing in timings.iter(){
                li {"{timing.name}: " if let Some(elapsed)=timing.elapsed{
                    {format!("{:.2}s",elapsed.as_secs_f64())}
                }else if prep.state()()==UseResourceState::Pending {"in progress"}else{"interrupted"}}
            }}
        }
    }}
}
#[derive(Clone, Copy)]
struct LookupMessage(Signal<Option<(bool, String)>>);

#[component]
pub fn SourcePane(mut reading: Signal<bool>, dark: bool, head: String) -> Element {
    let mut source = use_context::<Signal<Option<Source>>>();
    let mut history = use_signal(Vec::<Source>::new);
    rsx! {
            if let Some(view)=source.read().as_ref(){
                section {class:"definition-view",aria_label:"Definition source",onkeydown:move |e|e.stop_propagation(),
                    div {button {onmounted:move |e|async move{let _=e.set_focus(true).await;},
                        onclick:move |_|{reading.set(false);source.set(None);crate::focus_reader()},"Back to diff"}
                        if !history.read().is_empty(){
                            button {onclick:move |_|{if let Some(previous)=history.write().pop(){source.set(Some(previous));}},"Previous definition"}
                        }
                        h2 {"{view.path}"} span {"Read-only · Line {view.line+1}"} LookupStatus {}
                    }
                    for location in [(view.path.clone(),view.line)] {
                    NavigableDiff {key:"{location:?}",path:view.path.clone(),head:head.clone(),reading,
                        on_follow:move |_|{if let Some(previous)=source.write().take(){history.write().push(previous);}},
                    pre { tabindex:"0",onmounted:move |_|{document::eval(
                        "const line = document.querySelector('.definition-line'), pane = line?.closest('pre');
                         if (pane) pane.scrollTop += line.getBoundingClientRect().top - pane.getBoundingClientRect().top - pane.clientHeight / 2;"
                    );},
                        {source_rows(view,dark)}
                    }
                    }}
                }
            }
    }
}

fn source_rows(view: &Source, dark: bool) -> Element {
    use syntect::easy::HighlightLines;
    let syntax = Path::new(&view.path)
        .extension()
        .and_then(|s| s.to_str())
        .and_then(|ext| crate::diff::SYNTAX.find_syntax_by_extension(ext));
    let mut painter = syntax.map(|s| HighlightLines::new(s, crate::diff::theme(dark)));
    let rows=view.text.split_inclusive('\n').enumerate().map(|(i,line)|{
        let tokens=painter.as_mut().and_then(|p|p.highlight_line(line,&crate::diff::SYNTAX).ok());
        rsx!{div {class:if i==view.line {"definition-line"}else{""},
            span {class:"source-number","{i+1}"}
            span {"data-definition-line":supports(&view.path).then(||(i+1).to_string()),
            if let Some(tokens)=tokens{
                for (style,text) in tokens{
                    span {style:"color:rgb({style.foreground.r},{style.foreground.g},{style.foreground.b})",{text.trim_end_matches(['\r','\n'])}}
                }
            }else{{line.trim_end_matches(['\r','\n'])}}
            }
        }}
    });
    rsx! {for row in rows{{row}}}
}
type ResultOf<T> = Result<T, String>;
#[derive(Clone)]
struct Navigation(Rc<Workspace>);
impl PartialEq for Navigation {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
type PrivateDir = Rc<tempfile::TempDir>;
struct Workspace {
    warm: Option<(String, String)>,
    client: futures_util::lock::Mutex<Option<Server>>,
    server: String,
    prepared: Shared<LocalBoxFuture<'static, ResultOf<Option<PrivateDir>>>>,
}
async fn timed_phase<T>(
    mut phase: Signal<Vec<PhaseTiming>>,
    name: &'static str,
    work: impl Future<Output = T>,
) -> T {
    let start = std::time::Instant::now();
    {
        let mut timings = phase.write();
        if name == "Creating local checkout" {
            timings.clear();
        }
        timings.push(PhaseTiming {
            name,
            started: start,
            elapsed: None,
        });
    }
    eprintln!("[PR source] {name}…");
    let result = work.await;
    let elapsed = start.elapsed();
    if let Some(entry) = phase.write().iter_mut().find(|p| p.started == start) {
        entry.elapsed = Some(elapsed);
    }
    eprintln!("[PR source] {name}: {:.2}s", elapsed.as_secs_f64());
    result
}
async fn prepare(
    root: &str,
    head: &str,
    pr: &str,
    fetch: bool,
    phase: Signal<Vec<PhaseTiming>>,
) -> ResultOf<Option<PrivateDir>> {
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid PR commit.".into());
    }
    let dir = std::rc::Rc::new(
        tempfile::Builder::new()
            .prefix("ferriscope-pr-")
            .tempdir()
            .map_err(|e| e.to_string())?,
    );
    timed_phase(
        phase,
        "Creating local checkout",
        init_private_repo(Path::new(root), dir.path()),
    )
    .await?;
    let commit = format!("{head}^{{commit}}");
    if git(dir.path(), &["cat-file", "-e", &commit]).await.is_err() {
        if !fetch {
            return Ok(None);
        }
        let (repo, number) = pr.split_once("/pull/").ok_or("Invalid PR key.")?;
        timed_phase(
            phase,
            "Fetching PR commit",
            git(
                dir.path(),
                &[
                    "-c",
                    "credential.helper=",
                    "-c",
                    "credential.helper=!gh auth git-credential",
                    "fetch",
                    "--depth=1",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--no-write-fetch-head",
                    &format!("https://github.com/{repo}.git"),
                    &format!("refs/pull/{number}/head"),
                ],
            ),
        )
        .await
        .map_err(|_| "Could not fetch this PR. Check your GitHub CLI login and connection.")?;
        git(dir.path(),&["cat-file","-e",&commit]).await.map_err(|_|"The fetched PR does not contain the displayed commit. Refresh the PR and retry.")?;
    }
    timed_phase(
        phase,
        "Checking out PR files",
        git(dir.path(), &["checkout", "--detach", head]),
    )
    .await
    .map_err(|_| "Could not prepare the private PR checkout.")?;
    // The private checkout belongs to this reader and is never edited by
    // Ferriscope. Verify it once; individual files are still checked against
    // their pinned Git blobs when used.
    timed_phase(phase, "Verifying checkout", verify(dir.path(), head)).await?;
    Ok(Some(dir))
}

// A definition lookup needs one pinned commit, not a clone of every ref.
// Borrow only the trusted repository's object database. The index, refs,
// fetched objects and working files all belong to our temporary repository.
async fn init_private_repo(root: &Path, dest: &Path) -> ResultOf<()> {
    let objects = git(
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
    )
    .await?;
    let line = objects.strip_suffix(b"\n").unwrap_or(&objects);
    if line.is_empty()
        || line.contains(&b'\n')
        || line.contains(&b'\r')
        || !Path::new(
            std::str::from_utf8(line).map_err(|_| "Unsupported Git object directory path.")?,
        )
        .is_absolute()
    {
        return Err("Cannot share this Git object directory.".into());
    }
    let format = git(root, &["rev-parse", "--show-object-format"]).await?;
    let format = match format.trim_ascii() {
        b"sha1" => "--object-format=sha1",
        b"sha256" => "--object-format=sha256",
        _ => return Err("Unsupported Git object format.".into()),
    };
    git(dest, &["init", "--quiet", "--template=", format, "."]).await?;
    std::fs::write(dest.join(".git/objects/info/alternates"), objects)
        .map_err(|e| format!("Cannot share the local Git objects: {e}"))
}
#[cfg(all(test, unix))]
#[path = "../tests/definition/client.rs"]
mod tests;

async fn git(root: &Path, args: &[&str]) -> ResultOf<Vec<u8>> {
    let out = async_process::Command::new("git")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "core.hooksPath=/dev/null"])
        .arg("--no-optional-locks")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Cannot verify this file in the selected Git checkout.".into());
    }
    Ok(out.stdout)
}
async fn verify(root: &Path, head: &str) -> ResultOf<()> {
    let prefix = git(root, &["rev-parse", "--show-prefix"]).await?;
    if !prefix.trim_ascii().is_empty() {
        return Err("Select the repository root, not a subdirectory.".into());
    }
    if git(root, &["rev-parse", "--verify", "HEAD"])
        .await?
        .trim_ascii()
        != head.as_bytes()
    {
        return Err(format!(
            "Definitions unavailable: the local checkout is not at PR commit {}. Your diff is unchanged.",
            &head[..head.len().min(12)]
        ));
    }
    // Git status intentionally ignores edits to these tracked files. Don't
    // present that result as proof that the server sees the PR revision.
    if git(root, &["ls-files", "-v", "-z"])
        .await?
        .split(|b| *b == 0)
        .filter_map(|entry| entry.first())
        .any(|flag| *flag == b'S' || flag.is_ascii_lowercase())
    {
        return Err("Use a full checkout without skip-worktree or assume-unchanged files.".into());
    }
    if !git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )
    .await?
    .is_empty()
    {
        return Err(
            "The checkout has local changes. Select a clean checkout at the PR head.".into(),
        );
    }
    Ok(())
}
async fn tracked_text(root: &Path, path: &Path, head: &str) -> ResultOf<(String, String)> {
    let real = path
        .canonicalize()
        .map_err(|_| "File is missing in the checkout.")?;
    let rel = real
        .strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .ok_or("Definition is outside the trusted checkout.")?;
    if real != path || !real.is_file() {
        return Err("Only regular files inside the checkout are supported (no symlinks).".into());
    }
    let data = git(root, &["show", &format!("{head}:{rel}")]).await?;
    let text = String::from_utf8(data).map_err(|_| "Source is not UTF-8 text.")?;
    let disk = std::fs::read_to_string(&real)
        .map_err(|_| "Cannot read working-tree source as UTF-8 text.")?;
    if disk != text && disk.replace("\r\n", "\n") != text.replace("\r\n", "\n") {
        return Err("Working-tree source differs from the PR revision.".into());
    }
    Ok((rel.to_owned(), text))
}
async fn lookup(
    client: &futures_util::lock::Mutex<Option<Server>>,
    root: &Path,
    server: &str,
    head: &str,
    path: &str,
    s: &Selection,
) -> ResultOf<Source> {
    tokio::time::timeout(
        Duration::from_secs(25),
        resolve(client, root, server, head, path, s),
    )
    .await
    .map_err(|_| "Definition lookup timed out.".to_string())?
}
async fn resolve(
    client: &futures_util::lock::Mutex<Option<Server>>,
    root: &Path,
    server: &str,
    head: &str,
    path: &str,
    s: &Selection,
) -> ResultOf<Source> {
    if !Path::new(root).is_absolute() {
        return Err("Enter an absolute checkout path.".into());
    }
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|_| "Checkout directory not found.")?;
    let file = root.join(path);
    let (_, text) = tracked_text(&root, &file, head).await?;
    let line = s.line.checked_sub(1).ok_or("Invalid diff line.")?;
    let selected = format!("{}{}{}", s.before, s.symbol, s.after);
    if text.lines().nth(line) != Some(selected.trim_end_matches('\r')) {
        return Err("The selected diff line does not match the checkout.".into());
    }
    let mut slot = client.lock().await;
    // Keep the transport out of the cache while using it: cancellation,
    // timeout or a broken pipe drops the child rather than reusing partial IO.
    let mut live = match slot.take() {
        Some(live) => live,
        None => Server::start(&root, server).await?,
    };
    let result = live.definition(&file, path, text, line, s).await?;
    *slot = Some(live);
    drop(slot);
    read_definition(&root, head, result).await
}
struct Server {
    rpc: Rpc,
    _child: async_process::Child,
    encoding: String,
    opened: std::collections::HashSet<String>,
    next_id: u32,
}
impl Server {
    async fn start(root: &Path, server: &str) -> ResultOf<Self> {
        // Never resolve a relative executable against the selected repository.
        if !Path::new(server).is_absolute() && (server.contains('/') || server.contains('\\')) {
            return Err("Use an absolute executable path or a command on PATH.".into());
        }
        let mut child = async_process::Command::new(server)
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Cannot start language server {server}: {e}"))?;
        let mut rpc = Rpc {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
        };
        let root_uri = file_uri(&root)?;
        let init = rpc
            .request(
                1,
                "initialize",
                json!({
                    "processId":std::process::id(),
                    "rootUri":root_uri,
                    "workspaceFolders":[{"uri":root_uri,"name":"checkout"}],
                    "capabilities":{
                        "general":{"positionEncodings":["utf-16","utf-8","utf-32"]},
                        "textDocument":{"definition":{"linkSupport":true}}
                    }
                }),
            )
            .await?;
        let encoding = init
            .pointer("/capabilities/positionEncoding")
            .and_then(Value::as_str)
            .unwrap_or("utf-16")
            .to_owned();
        rpc.notify("initialized", json!({})).await?;
        // A cold vtsls syntax worker can resolve an alias only as far as its local
        // import. This one-shot client needs the semantic worker's definition.
        if init.pointer("/serverInfo/name").and_then(Value::as_str) == Some("vtsls") {
            rpc.notify(
                "workspace/didChangeConfiguration",
                json!({"settings":{"typescript":{"tsserver":{"useSyntaxServer":"never"}}}}),
            )
            .await?;
        }
        Ok(Self {
            rpc,
            _child: child,
            encoding,
            opened: Default::default(),
            next_id: 2,
        })
    }
    async fn definition(
        &mut self,
        file: &Path,
        path: &str,
        text: String,
        line: usize,
        s: &Selection,
    ) -> ResultOf<Value> {
        let uri = file_uri(file)?;
        let column = match self.encoding.as_str() {
            "utf-16" => s.before.encode_utf16().count(),
            "utf-8" => s.before.len(),
            "utf-32" => s.before.chars().count(),
            _ => return Err("Unsupported server position encoding.".into()),
        };
        self.open(file, path, text).await?;
        let id = self.next_id;
        self.next_id += 1;
        self.rpc
            .request(
                id,
                "textDocument/definition",
                json!({"textDocument":{"uri":uri},"position":{"line":line,"character":column}}),
            )
            .await
    }
    async fn open(&mut self, file: &Path, path: &str, text: String) -> ResultOf<()> {
        let uri = file_uri(file)?;
        if self.opened.insert(uri.clone()) {
            let language = if path.ends_with(".tsx") {
                "typescriptreact"
            } else {
                "typescript"
            };
            self.rpc
                .notify(
                    "textDocument/didOpen",
                    json!({
                        "textDocument":{"uri":uri,"version":1,"languageId":language,"text":text}
                    }),
                )
                .await?;
        }
        Ok(())
    }
}
async fn read_definition(root: &Path, head: &str, result: Value) -> ResultOf<Source> {
    let loc = match &result {
        Value::Null => None,
        Value::Array(v) if v.len() > 1 => {
            return Err(
                "Multiple definitions returned; choosing among them is not supported yet.".into(),
            );
        }
        Value::Array(v) => v.first(),
        v => Some(v),
    }
    .ok_or("No definition found for this symbol.")?;
    let target = loc
        .get("uri")
        .or_else(|| loc.get("targetUri"))
        .and_then(Value::as_str)
        .and_then(|s| Url::parse(s).ok())
        .and_then(|u| u.to_file_path().ok())
        .ok_or("Only local file definitions are supported.")?;
    let line = loc
        .pointer("/targetSelectionRange/start/line")
        .or_else(|| loc.pointer("/range/start/line"))
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("Invalid definition location.")?;
    let (path, text) = tracked_text(&root, &target, head).await?;
    if line >= text.split('\n').count() {
        return Err("Definition line is outside the target file.".into());
    }
    Ok(Source { path, text, line })
}
fn file_uri(path: &Path) -> ResultOf<String> {
    Url::from_file_path(path)
        .map(String::from)
        .map_err(|_| "Invalid file path.".into())
}

struct Rpc {
    input: async_process::ChildStdin,
    output: BufReader<async_process::ChildStdout>,
}
impl Rpc {
    async fn send(&mut self, value: Value) -> ResultOf<()> {
        let body = value.to_string();
        self.input
            .write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.input.flush().await.map_err(|e| e.to_string())
    }
    async fn notify(&mut self, method: &str, params: Value) -> ResultOf<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
            .await
    }
    async fn read(&mut self) -> ResultOf<Value> {
        let mut size = None;
        loop {
            let mut header = String::new();
            if self
                .output
                .read_line(&mut header)
                .await
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("Language server closed its output.".into());
            }
            if header.trim().is_empty() {
                break;
            }
            if let Some((name, n)) = header.split_once(':')
                && name.eq_ignore_ascii_case("Content-Length")
            {
                size = n.trim().parse::<usize>().ok();
            }
        }
        let mut buf = vec![
            0;
            size.filter(|s| *s <= 16 * 1024 * 1024)
                .ok_or("Invalid language server message size.")?
        ];
        self.output
            .read_exact(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&buf).map_err(|e| format!("Invalid language server response: {e}"))
    }
    async fn request(&mut self, id: u32, method: &str, params: Value) -> ResultOf<Value> {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        loop {
            let msg = self.read().await?;
            if msg.get("method").is_some() {
                if let Some(id) = msg.get("id") {
                    self.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Unsupported client request"}})).await?
                }
            } else if msg["id"] == id {
                if let Some(err) = msg.get("error") {
                    return Err(format!("Language server: {}", err["message"]));
                }
                return msg
                    .get("result")
                    .cloned()
                    .ok_or("Missing language server result.".into());
            }
        }
    }
}
