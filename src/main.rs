mod platform;
use dioxus::prelude::*;
use ferriscope::{File, KINDS, Review, hash, kind, pr_key};
use std::collections::{HashMap, HashSet};

fn main() {
    dioxus::launch(App)
}

#[component]
fn App() -> Element {
    let mut url = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut session = use_signal(|| None::<(String, Vec<File>, Review)>);
    let mut revision = use_signal(|| 0usize);
    let open = move |event: FormEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let key = match pr_key(&url.read()) {
            Ok(k) => k,
            Err(e) => {
                error.set(e);
                return;
            }
        };
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            let result = async {
                let files = platform::fetch(&key).await?;
                let mut review = platform::load(&key)?;
                review.reconcile(&files);
                platform::save(&key, &review)?;
                Ok::<_, String>((key, files, review))
            }
            .await;
            match result {
                Ok(s) => {
                    session.set(Some(s));
                    revision += 1
                }
                Err(e) => error.set(e),
            }
            busy.set(false);
        });
    };
    rsx! {
        document::Title {"Ferriscope · PR reader"}
        style { {include_str!("style.css")} }
        header {strong {"Ferriscope"} small {"PERSONAL PR READER"}
            form {onsubmit:open,
                input {aria_label:"GitHub PR URL",placeholder:"https://github.com/owner/repo/pull/123",value:"{url}",oninput:move|e|url.set(e.value())}
                button {disabled:busy(), if busy() {"Loading…"} else {"Open / refresh"}}
            }
        }
        if !error.read().is_empty(){p {role:"alert","{error}"}}
        if let Some((pr,files,review))=session.read().as_ref(){
            Reader {key:"{revision}",pr:pr.clone(), files:files.clone(),initial:review.clone()}
        }else{section {class:"intro",small {"REVIEW WORKSPACE"} h1 {"Read changes by purpose."} p {"Paste a pull request above. Start with implementation and tests; lockfiles stay folded away."}}}
    }
}

#[component]
fn Reader(pr: String, files: Vec<File>, initial: Review) -> Element {
    let mut state = use_signal(|| initial.clone());
    let index = use_hook(|| {
        files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.filename.clone(), i))
            .collect::<HashMap<_, _>>()
    });
    let mut active = use_signal(|| {
        initial
            .groups
            .iter()
            .filter(|g| g.name != KINDS[4])
            .flat_map(|g| &g.files)
            .next()
            .and_then(|p| index.get(p))
            .copied()
    });
    let mut selected = use_signal(HashSet::<String>::new);
    let mut name = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut folded = use_signal(|| HashSet::from([KINDS[4].to_string()]));
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
    let groups = state.read().groups.clone();
    rsx! {
        main {
            aside {
                small {"PULL REQUEST"} h3 {"{pr}"} p {"{files.len()} changed files"}
                if !selected.read().is_empty(){div {class:"organize",
                    input {aria_label:"Group name",placeholder:"Group name",value:"{name}",oninput:move|e|name.set(e.value())}
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
                            button {class:"label",aria_expanded:!folded.read().contains(&g.name),onclick:{
                                let n=g.name.clone();move |_|{let mut f=folded.write();if !f.remove(&n){f.insert(n.clone());}}
                            },if folded.read().contains(&g.name) {"▸ "}else{"▾ "} "{g.name} · {g.files.len()}"}
                            Order {pos:gi,len:groups.len(),onmove:move |other|{let mut s=state();s.groups.swap(gi,other);save.call(s)}}
                        }
                        if !folded.read().contains(&g.name){
                            for (fi,path) in g.files.iter().enumerate(){
                                if let Some(&idx)=index.get(path){
                                    div {key:"{path}",class:if active()==Some(idx) {"row current"}else{"row"},
                                        input {r#type:"checkbox",aria_label:"Select {path}",checked:selected.read().contains(path),onchange:{
                                            let p=path.clone();move |_|{let mut set=selected.write();if !set.remove(&p){set.insert(p.clone());}}
                                        }}
                                        button {class:"label",onclick:move |_|active.set(Some(idx)),"{path}"
                                            span {class:"badge","{kind(path)} · {state.read().status(&files[idx])}"}
                                        }
                                        Order {pos:fi,len:g.files.len(),onmove:move |other|{let mut s=state();s.groups[gi].files.swap(fi,other);save.call(s)}}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            article {
                if let Some(f)=active().and_then(|i|files.get(i)){
                    small {"UNIFIED DIFF · {f.status}"} h2 {"{f.filename}"}
                    div {class:"tools",
                        a {href:"https://github.com/{pr}/files#diff-{hash(&f.filename)}",target:"_blank",rel:"noopener noreferrer","GitHub diff ↗"}
                        button {onclick:{let f=f.clone();move |_|{let mut s=state();s.toggle(&f);save.call(s)}},
                            if state.read().status(f)=="Reviewed" {"Mark unread"}else{"Mark reviewed"}
                        }
                        b {"{state.read().status(f)}"}
                    }
                    if let Some(patch)=&f.patch{
                        pre {for line in patch.lines(){div {class:match line.as_bytes().first(){Some(b'+')=>"add",Some(b'-')=>"del",Some(b'@')=>"hunk",_=>""},"{line}"} }}
                    } else {p {"GitHub supplied no text patch for this file. Open the GitHub diff for details."}}
                }else{h2 {"Select a file to read its diff."}}
            }
        }
    }
}
#[component]
fn Order(pos: usize, len: usize, onmove: EventHandler<usize>) -> Element {
    rsx! {div {class:"order",
        button {title:"Move up",disabled:pos==0,onclick:move |_|onmove.call(pos-1),"↑"}
        button {title:"Move down",disabled:pos+1>=len,onclick:move |_|onmove.call(pos+1),"↓"}
    }}
}
