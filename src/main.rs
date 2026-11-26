mod comments;
mod diff;
#[cfg(not(target_arch = "wasm32"))]
mod gh;
#[cfg(feature = "desktop")]
mod inbox;
mod platform;
#[cfg(not(target_arch = "wasm32"))]
mod preview;
mod updates;
use dioxus::prelude::*;
use ferriscope::{File, Review, hash, kind, pr_key};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

fn main() {
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, WindowBuilder, icon_from_memory};
        let window = WindowBuilder::new().with_title("Ferriscope");
        let icon = icon_from_memory(include_bytes!("../assets/ferriscope.png"))
            .expect("valid embedded Ferriscope icon");
        LaunchBuilder::desktop()
            .with_cfg(Config::new().with_window(window).with_icon(icon))
            .launch(App);
    }
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App)
}

#[component]
fn App() -> Element {
    let theme = use_signal(platform::load_theme);
    let mut url = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut session = use_signal(|| None::<(platform::Snapshot, Review)>);
    let mut show_reader = use_signal(|| false);
    let mut revision = use_signal(|| 0usize);
    let mut recents = use_signal(platform::load_recents);
    let mut show_recents = use_signal(|| false);
    #[cfg(feature = "desktop")]
    use_effect(move || {
        if show_reader() {
            focus_reader();
        }
    });
    let open = use_callback(move |input: String| {
        if busy() {
            return;
        }
        let key = match pr_key(&input) {
            Ok(k) => k,
            Err(e) => {
                error.set(e);
                return;
            }
        };
        busy.set(true);
        url.set(input);
        show_recents.set(false);
        error.set(String::new());
        let cached = session
            .peek()
            .as_ref()
            .map(|(snapshot, _)| snapshot.clone());
        let dark = theme.peek().as_str() != "light";
        spawn(async move {
            let result = async {
                let snapshot = platform::fetch(&key, cached, async |files| {
                    diff::prepare_first(files, dark).await
                })
                .await?;
                let mut review = platform::load(&key)?;
                review.reconcile(&snapshot.files);
                platform::save(&key, &review)?;
                Ok::<_, String>((snapshot, review))
            }
            .await;
            match result {
                Ok(s) => {
                    let mut list = recents();
                    list.retain(|key| key != &s.0.key);
                    list.insert(0, s.0.key.clone());
                    list.truncate(10);
                    if let Err(e) = platform::save_recents(&list) {
                        error.set(format!("Could not save recent PRs: {e}"))
                    }
                    recents.set(list);
                    session.set(Some(s));
                    show_reader.set(true);
                    revision += 1
                }
                Err(e) => error.set(e),
            }
            busy.set(false);
        });
    });
    rsx! {
        document::Title {"Ferriscope · PR reader"}
        style { {include_str!("style.css")} }
        div {class:"workspace", "data-theme":"{theme}",
        header {Appearance {theme,error} small {"PR READER"}
            if cfg!(feature = "desktop") && session.read().is_some() {
                button {disabled:busy(),onclick:move |_|{
                    show_recents.set(false);
                    show_reader.toggle();
                },if show_reader() {"Home"} else {"Back to reader"}}
            }
            form {onsubmit:move |event|{event.prevent_default();let input=url();open.call(input)},
                input {aria_label:"GitHub PR URL",placeholder:"https://github.com/owner/repo/pull/123",value:"{url}",oninput:move|e|url.set(e.value())}
                button {disabled:busy(), if busy() {"Loading…"} else {"Open / refresh"}}
            }
            div {class:"recent-prs",onkeydown:move|e|if e.key()==Key::Escape{show_recents.set(false);document::eval("document.getElementById('recent-toggle')?.focus()");},
                button {id:"recent-toggle",aria_expanded:show_recents(),disabled:busy(),onclick:move |_|show_recents.toggle(),"Recent PRs"}
                if show_recents(){
                    section {class:"recent-panel",aria_label:"Recently opened pull requests",
                        if recents.read().is_empty(){p {"No recently opened PRs yet."}}
                        for key in recents.read().iter(){
                            button {key:"{key}",onclick:{let link=format!("https://github.com/{key}");move |_|open.call(link.clone())},"{key}"}
                        }
                        if !recents.read().is_empty(){button {class:"clear-history",onclick:move |_|match platform::save_recents(&[]){Ok(())=>recents.set(Vec::new()),Err(e)=>error.set(e)},"Clear history"}}
                    }
                }
            }
        }
        if !error.read().is_empty(){p {role:"alert","{error}"}}
        if let Some((snapshot,review))=session.read().as_ref(){
            // Keep the reader mounted while visiting the inbox so navigation
            // and in-progress review input aren't reset by the trip home.
            div {hidden:!show_reader(),
                Reader {key:"{revision}",pr:snapshot.key.clone(),head:snapshot.head().to_owned(), files:snapshot.files.clone(),initial:review.clone(),dark:theme.read().as_str()!="light"}
            }
        }
        if !show_reader(){
            {home(open,busy())}
        }
        }
    }
}

fn home(_open: Callback<String>, _busy: bool) -> Element {
    #[cfg(feature = "desktop")]
    return rsx! { inbox::YourPrs {open:_open,busy:_busy} };
    #[cfg(not(feature = "desktop"))]
    rsx! {
        section {class:"intro",
            small {"PERSONAL REVIEW WORKSPACE"}
            h1 {"Read changes by purpose."}
            p {"Paste a pull request above to explore its files, bring related changes together, and keep track of what you’ve reviewed."}
        }
    }
}

#[component]
fn Appearance(mut theme: Signal<String>, mut error: Signal<String>) -> Element {
    rsx! {
            details {class:"preferences",
                summary {class:"brand",aria_label:"Ferriscope settings",title:"Ferriscope settings",
                    span {class:"mark",aria_hidden:"true",dangerous_inner_html:include_str!("../assets/ferriscope.svg")}
                    strong {"Ferriscope"} span {aria_hidden:"true","▾"}
                }
                div {class:"settings-panel",
                    div {role:"group",aria_labelledby:"theme-title",
                    strong {id:"theme-title","Color theme"}
                    for (id,label) in [("light","Light"),("dark","Dark"),("dusk","Dusk")] {
                        label {
                            input {r#type:"radio",name:"theme",checked:theme.read().as_str()==id,onchange:move |_|{
                                if let Err(msg)=platform::save_theme(id){error.set(msg)}
                                theme.set(id.into());
                            }}
                            "{label}"
                        }
                    }
                    }
                    updates::UpdateCheck {}
                }
            }
    }
}

#[component]
fn Reader(pr: String, head: String, files: Rc<Vec<File>>, initial: Review, dark: bool) -> Element {
    use_context_provider(|| Rc::new(std::cell::RefCell::new(diff::HighlightCache::default())));
    let mut state = use_signal(|| initial.clone());
    let index = use_hook(|| {
        Rc::new(
            files
                .iter()
                .enumerate()
                .map(|(i, f)| (f.filename.clone(), i))
                .collect::<HashMap<_, _>>(),
        )
    });
    let mut active = use_signal(|| None::<usize>);
    let mut folders = use_signal(HashSet::<(String, String)>::new);
    // An expanded kind can contain thousands of files. Keep its full logical
    // order for keyboard navigation, but mount the sidebar in small batches.
    const FILE_BATCH: usize = 60;
    let mut pages = use_signal(HashMap::<String, usize>::new);
    let mut query = use_signal(String::new);
    let mut search_limit = use_signal(|| FILE_BATCH);
    let paths = use_hook(|| {
        Rc::new(
            files
                .iter()
                .map(|f| f.filename.to_lowercase())
                .collect::<Vec<_>>(),
        )
    });
    let mut anchor = use_signal(|| None::<comments::Anchor>);
    use_effect(move || {
        let _ = active();
        document::eval(
            "const diff = document.querySelector('.diff-scroll'); if (diff) diff.scrollTop = 0;",
        );
    });
    use_effect(move || {
        if anchor.read().is_some() {
            // A narrow window stacks the file list above the reader; bring the
            // reader into view, then reveal the line without scrolling code sideways.
            document::eval(
                "const diff = document.querySelector('.diff-scroll');
                 const line = diff?.querySelector('.comment-target');
                 if (line) {
                     diff.closest('.reader').scrollIntoView({block:'nearest', inline:'nearest'});
                     const viewport = diff.getBoundingClientRect();
                     const target = line.getBoundingClientRect();
                     if (target.top < viewport.top) diff.scrollTop += target.top - viewport.top;
                     else if (target.bottom > viewport.bottom) diff.scrollTop += target.bottom - viewport.bottom;
                 }",
            );
        }
    });
    let mut selected = use_signal(HashSet::<String>::new);
    let mut name = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut expanded = use_signal(HashSet::<String>::new);
    let storage_key = use_hook(|| pr.clone());
    let save = use_callback(
        move |next: Review| match platform::save(&storage_key, &next) {
            Ok(()) => {
                state.set(next);
                error.set(String::new())
            }
            Err(e) => error.set(e),
        },
    );
    let review = state.read();
    let groups = &review.groups;
    let toggle = use_callback({
        let snapshot = Rc::clone(&files);
        move |()| {
            if let Some(i) = active() {
                let mut s = state();
                s.toggle(&snapshot[i]);
                save.call(s);
            }
        }
    });
    let ordered: Vec<_> = groups
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            let index = &index;
            group
                .files
                .iter()
                .enumerate()
                .filter_map(move |(fi, p)| index.get(p).map(|&i| (g, fi, i)))
        })
        .collect();
    let needle = query.read().trim().to_lowercase();
    let matches: Vec<_> = if needle.is_empty() {
        Vec::new()
    } else {
        ordered
            .iter()
            .enumerate()
            .filter_map(|(p, &(_, _, i))| paths[i].contains(&needle).then_some((p, i)))
            .collect()
    };
    let first = matches.first().map(|&(p, _)| p);
    let pos = ordered.iter().position(|&(_, _, i)| Some(i) == active());
    let previous = pos.and_then(|p| p.checked_sub(1));
    let next = Some(pos.map_or(0, |p| p + 1)).filter(|&p| p < ordered.len());
    let jump = use_callback(move |p: usize| {
        let (g, fi, i) = ordered[p];
        let review = state.read();
        let group = &review.groups[g];
        expanded.write().insert(group.name.clone());
        let page = fi / FILE_BATCH;
        if pages.read().get(&group.name).copied().unwrap_or(0) != page {
            pages.write().insert(group.name.clone(), page);
        }
        let path = &group.files[fi];
        folders
            .write()
            .remove(&(group.name.clone(), directory(path).into()));
        active.set(Some(i));
        focus_reader();
    });
    let choose = use_callback(move |p: usize| {
        query.set(String::new());
        jump.call(p)
    });
    let file = active().and_then(|i| files.get(i));
    rsx! {
        main { tabindex:"-1",
            onmounted:move |e|async move{let _=e.set_focus(true).await;},
            onkeydown:move |e:KeyboardEvent|{
                if !e.modifiers().is_empty(){return}
                if e.key()==Key::Character("/".into()){e.prevent_default();document::eval("document.getElementById('file-search')?.focus()");return}
                if e.key()==Key::Character(" ".into()) && active().is_some(){e.prevent_default();if !e.is_auto_repeating(){toggle.call(())}return}
                let dest=match e.key(){Key::Character(k) if k=="j"=>next,Key::Character(k) if k=="k"=>previous,_=>return};
                if let Some(p)=dest{e.prevent_default();jump.call(p)}
            },
            aside {onkeydown:keep_native_space,
                small {"PULL REQUEST"} h3 {"{pr}"} p {"{files.len()} changed files"}
                input {id:"file-search",r#type:"search",aria_label:"Search files",placeholder:"Search files by path  /",value:"{query}",
                    oninput:move |e|{query.set(e.value());search_limit.set(FILE_BATCH)},
                    onkeydown:move |e:KeyboardEvent|{
                        e.stop_propagation();
                        match e.key(){
                            Key::Escape=>{e.prevent_default();query.set(String::new());focus_reader();}
                            Key::Enter=>{if let Some(p)=first{e.prevent_default();choose.call(p)}}
                            _=>{}
                        }
                    }
                }
                if !needle.is_empty(){
                    small {role:"status","{matches.len()} matching files"}
                    if matches.is_empty(){p {"No files match this path. Try a shorter name or press Esc to clear."}}
                    for &(p,i) in matches.iter().take(search_limit()){
                        button {class:"search-result",key:"{i}",onclick:move |_|choose.call(p),
                            "{files[i].filename}"
                            span {class:"file-meta",
                                span {class:"badge","{kind(&files[i].filename)}"}
                                {line_counts(&files[i])}
                            }
                        }
                    }
                    if matches.len()>search_limit(){button {onclick:move |_|search_limit+=FILE_BATCH,"Show more matches"}}
                }else{
                small {"FILE GROUPS"}
                if !selected.read().is_empty(){div {class:"organize",
                    input {aria_label:"Group name",placeholder:"Group name",value:"{name}",onkeydown:move|e|e.stop_propagation(),oninput:move|e|name.set(e.value())}
                    button {disabled:selected.read().len()<2,onclick:move |_|{
                        let mut s=state();
                        let result=s.together(&name.read(),&selected.read());
                        match result{ Ok(())=>{save.call(s);selected.write().clear();name.set(String::new());},Err(e)=>error.set(e)}
                    },"Review together ({selected.read().len()})"}
                }}
                if !error.read().is_empty(){p {role:"alert","{error}"}}
                for (gi,g) in groups.iter().enumerate(){
                    section {key:"{g.name}",
                        div {class:"row heading",
                            button {class:"label",aria_expanded:expanded.read().contains(&g.name),onclick:{
                                let n=g.name.clone();move |_|{let mut f=expanded.write();if !f.remove(&n){f.insert(n.clone());}}
                            }, span {class:"chevron",aria_hidden:"true","›"} "{g.name}" span {class:"count","{g.files.len()}"}}
                            Order {pos:gi,len:groups.len(),onmove:move |other|{let mut s=state();s.groups.swap(gi,other);save.call(s)}}
                            if g.custom {
                                button {aria_label:"Remove group {g.name}",title:"Return files to their default groups; keep review progress",
                                    onclick:{let name=g.name.clone();let files=Rc::clone(&files);move |_|{
                                        let mut s=state();
                                        match s.remove_group(&name,&files){
                                            Ok(())=>{save.call(s);expanded.write().remove(&name);pages.write().remove(&name);}
                                            Err(e)=>error.set(e),
                                        }
                                    }},"Ungroup"}
                            }
                        }
                        if expanded.read().contains(&g.name){
                            {let page=pages.read().get(&g.name).copied().unwrap_or(0).min(g.files.len().saturating_sub(1)/FILE_BATCH);
                             let start=page*FILE_BATCH;
                             rsx!{
                            if page>0{
                                button {onclick:{let name=g.name.clone();move |_|{pages.write().insert(name.clone(),page-1);}},"Previous files"}
                            }
                            for (fi,path) in g.files.iter().enumerate().skip(start).take(FILE_BATCH){
                                if !g.custom && (fi==start || directory(&g.files[fi-1])!=directory(path)){
                                    button {class:"folder",aria_expanded:!folders.read().contains(&(g.name.clone(),directory(path).into())),onclick:{
                                        let k=(g.name.clone(),directory(path).to_owned());move |_|{let mut set=folders.write();if !set.remove(&k){set.insert(k.clone());}}
                                    },span {class:"chevron",aria_hidden:"true","›"} "{directory(path)} /"}
                                }
                                if g.custom || !folders.read().contains(&(g.name.clone(),directory(path).into())){
                                if let Some(&idx)=index.get(path){
                                    div {key:"{path}",class:if active()==Some(idx) {"row current"}else{"row"},
                                        input {r#type:"checkbox",aria_label:"Select {path}",checked:selected.read().contains(path),onchange:{
                                            let p=path.clone();move |_|{let mut set=selected.write();if !set.remove(&p){set.insert(p.clone());}}
                                        }}
                                        button {class:"label",aria_label:file_row_label(&files[idx]),title:"{path}",onclick:move |_|{active.set(Some(idx));focus_reader();},
                                            if g.custom {"{path}"}else{"{path.rsplit('/').next().unwrap_or(path)}"}
                                            span {class:"file-meta",
                                                span {class:"badge","{kind(path)} · {state.read().status(&files[idx])}"}
                                                {line_counts(&files[idx])}
                                            }
                                        }
                                        Order {pos:fi,len:g.files.len(),onmove:move |other|{let mut s=state();s.groups[gi].files.swap(fi,other);save.call(s)}}
                                    }
                                }
                                }
                            }
                            if g.files.len()>start+FILE_BATCH{
                                button {onclick:{let name=g.name.clone();move |_|{pages.write().insert(name.clone(),page+1);}},"Show more files"}
                            }
                            }}
                        }
                    }
                }
                }
            }
            article {class:"reader",
                div {class:"reader-top",
                    nav {class:"file-nav",aria_label:"File navigation",onkeydown:keep_native_space,
                        span { if let Some(p)=pos {"File {p+1} of {files.len()}"} else {"{files.len()} files"} }
                        button {disabled:previous.is_none(),onclick:move |_|if let Some(p)=previous{jump.call(p)}, "Previous " kbd {"K"}}
                        button {disabled:next.is_none(),onclick:move |_|if let Some(p)=next{jump.call(p)}, "Next " kbd {"J"}}
                    }
                    if let Some(f)=file {
                        div {class:"file-heading",
                            small {"UNIFIED DIFF · {f.change_type()}"} h2 {"{f.filename}"}
                            div {class:"tools",onkeydown:keep_native_space,
                                {line_counts(f)}
                                a {href:"https://github.com/{pr}/files#diff-{hash(&f.filename)}",target:"_blank",rel:"noopener noreferrer","GitHub diff ↗"}
                                button {title:"Toggle reviewed (Space)",onclick:move |_|toggle.call(()),
                                    if state.read().status(f)=="Reviewed" {"Mark unread"}else{"Mark reviewed"}
                                    " " kbd {"Space"}
                                }
                                b {"{state.read().status(f)}"}
                            }
                        }
                    }
                }
                div {class:"diff-scroll",
                    if let Some(f)=file {
                        if let Some(patch)=f.patch(){
                            diff::Diff {key:"{f.filename}",path:f.filename.clone(),patch:patch.clone(),dark,oncomment:move |a|anchor.set(Some(a)),anchor:anchor()}
                        } else {p {"GitHub supplied no text patch for this file. Open the GitHub diff for details."}}
                    }else{div {class:"empty",span {class:"diff-icon",aria_hidden:"true","±"} small {"DIFF VIEWER"} h2 {"Choose a file to begin"} p {"Expand a group and select a file to read its changes."}}}
                }
                {draft_tray(pr.clone(),head,anchor,dark)}
            }
        }
    }
}
fn describe_line_counts(added: u32, removed: u32) -> String {
    format!("Lines added: {added}; lines removed: {removed}")
}
fn file_row_label(file: &File) -> String {
    match file.line_changes() {
        Some((added, removed)) => {
            format!(
                "{} · {}",
                file.filename,
                describe_line_counts(added, removed)
            )
        }
        None => file.filename.clone(),
    }
}
fn line_counts(file: &File) -> Element {
    let Some((added, removed)) = file.line_changes() else {
        return rsx! {};
    };
    let description = describe_line_counts(added, removed);
    rsx! {
        span {class:"line-counts",role:"img",aria_label:"{description}",title:"{description}",
            span {class:"lines-added","+{added}"}
            span {class:"lines-removed","−{removed}"}
        }
    }
}
fn directory(path: &str) -> &str {
    path.rsplit_once('/')
        .map_or("Repository root", |(dir, _)| dir)
}
fn keep_native_space(e: KeyboardEvent) {
    if e.key() == Key::Character(" ".into()) {
        e.stop_propagation()
    }
}
fn focus_reader() {
    document::eval("document.querySelector('main')?.focus({preventScroll:true})");
}
fn draft_tray(
    _pr: String,
    _head: String,
    _anchor: Signal<Option<comments::Anchor>>,
    _dark: bool,
) -> Element {
    #[cfg(not(target_arch = "wasm32"))]
    return rsx! {comments::desktop::Tray {pr:_pr,head:_head,anchor:_anchor,dark:_dark}};
    #[cfg(target_arch = "wasm32")]
    rsx! {}
}
#[component]
fn Order(pos: usize, len: usize, onmove: EventHandler<usize>) -> Element {
    rsx! {div {class:"order",
        button {title:"Move up",disabled:pos==0,onclick:move |_|onmove.call(pos-1),"↑"}
        button {title:"Move down",disabled:pos+1>=len,onclick:move |_|onmove.call(pos+1),"↓"}
    }}
}
