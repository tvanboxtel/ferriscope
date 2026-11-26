use dioxus::prelude::*;
use std::sync::LazyLock;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

pub(crate) static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub(crate) fn theme(dark:bool)->&'static syntect::highlighting::Theme {
    &THEMES.themes[if dark {"base16-ocean.dark"}else{"InspiredGitHub"}]
}

#[component]
pub fn Diff(
    path: String,
    patch: std::rc::Rc<str>,
    dark: bool,
    oncomment: EventHandler<crate::comments::Anchor>,
    editing: bool,
) -> Element {
    const BATCH: usize = 250;
    let mut limit = use_signal(|| BATCH);
    let ext = path
        .rsplit('/')
        .next()
        .unwrap_or(&path)
        .rsplit('.')
        .next()
        .unwrap_or("");
    let syntax = SYNTAX.find_syntax_by_extension(ext);
    let theme = theme(dark);
    let fresh = || syntax.map(|s| HighlightLines::new(s, theme));
    let (mut old, mut new) = (fresh(), fresh());
    let mut rows = Vec::new();
    let mut lines = patch
        .split_inclusive('\n')
        .zip(crate::comments::coordinates(&patch));
    for (line, coord) in lines.by_ref().take(limit()) {
        let (class, lane) = match line.as_bytes()[0] {
            b'+' => ("add", &mut new),
            b'-' => ("del", &mut old),
            b' ' => {
                if let Some(h) = &mut old {
                    let _ = h.highlight_line(&line[1..], &SYNTAX);
                }
                ("", &mut new)
            }
            _ => {
                if line.starts_with("@@") {
                    (old, new) = (fresh(), fresh());
                }
                rows.push(rsx! { div { class:"hunk", {line.trim_end_matches('\n')} } });
                continue;
            }
        };
        let spans = lane
            .as_mut()
            .and_then(|h| h.highlight_line(&line[1..], &SYNTAX).ok());
        rows.push(rsx! { div { class,
            if cfg!(not(target_arch="wasm32")) {
                if let Some((side,n))=coord{
                    button {class:"line-comment",disabled:editing,title:"Draft a comment on {side} line {n}",onkeydown:crate::keep_native_space,
                        onclick:{let path=path.clone();move |_|oncomment.call(crate::comments::Anchor{path:path.clone(),side:side.into(),line:n})},"{n} +"}
                }
            }
            if let Some(tokens) = spans {
                {&line[..1]}
                for (style,text) in tokens {
                    span { style:"color:rgb({style.foreground.r},{style.foreground.g},{style.foreground.b})", {text.trim_end_matches('\n')} }
                }
            } else { {line.trim_end_matches('\n')} }
        }});
    }
    rsx! { pre { for row in rows { {row} } }
        if lines.next().is_some(){
            button {onclick:move |_|limit+=BATCH,"Show more lines"}
        }
    }
}
