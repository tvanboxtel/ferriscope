use dioxus::prelude::*;
use std::sync::LazyLock;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

pub(crate) static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub(crate) fn theme(dark: bool) -> &'static syntect::highlighting::Theme {
    &THEMES.themes[if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    }]
}

/// Syntect compiles grammar regexes on the first real line. Warm one likely
/// grammar, not an entire speculative diff. Actual rendering starts afresh.
pub async fn prepare_first(files: &[ferriscope::File], dark: bool) {
    use std::sync::Once;
    static WARM: Once = Once::new();
    if WARM.is_completed() {
        return;
    }
    let Some(file) = files
        .iter()
        .find(|f| f.patch().is_some() && ferriscope::kind(&f.filename) == "Implementation")
    else {
        return;
    };
    let ext = std::path::Path::new(&file.filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();
    let patch = file.patch().unwrap();
    // Bound even the scan for newlines: a minified source file might be one
    // huge line. Never try to warm its grammar using the entire line.
    let sample = patch[..patch.floor_char_boundary(patch.len().min(2048))].to_owned();
    let warm = move || {
        WARM.call_once(|| {
            let Some(syntax) = SYNTAX.find_syntax_by_extension(&ext) else {
                return;
            };
            let mut highlighter = HighlightLines::new(syntax, theme(dark));
            for line in sample.split_inclusive('\n').take(4) {
                if line.len() <= 512 && line.starts_with(['+', '-', ' ']) {
                    let _ = highlighter.highlight_line(&line[1..], &SYNTAX);
                }
            }
        })
    };
    #[cfg(target_arch = "wasm32")]
    {
        // Yield a task so the outstanding network request actually starts
        // before synchronous JS/WASM grammar initialization.
        gloo_timers::future::TimeoutFuture::new(8).await;
        warm();
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = tokio::task::spawn_blocking(warm).await;
}

const BATCH: usize = 250;

// Only retain offsets and colors, never UI nodes, handlers or duplicate patch text.
struct Token {
    len: usize,
    rgb: [u8; 3],
}
struct Row {
    len: usize,
    class: &'static str,
    coord: Option<(&'static str, u32)>,
    tokens: Vec<Token>,
}
struct Page {
    rows: Vec<Row>,
    more: bool,
}
struct Entry {
    patch: Rc<str>,
    path: String,
    dark: bool,
    page: Rc<Page>,
}
#[derive(Default)]
pub struct HighlightCache(VecDeque<Entry>);
impl HighlightCache {
    fn page(&mut self, path: &str, patch: &Rc<str>, dark: bool, limit: usize) -> Rc<Page> {
        if limit == BATCH
            && let Some(i) = self
                .0
                .iter()
                .position(|e| e.dark == dark && e.path == path && Rc::ptr_eq(&e.patch, patch))
        {
            let hit = self.0.remove(i).unwrap();
            let page = Rc::clone(&hit.page);
            self.0.push_front(hit);
            return page;
        }
        let page = Rc::new(highlight(path, patch, dark, limit));
        // Bound the retained token buffers even for minified or generated files.
        if limit == BATCH && page.rows.iter().map(|r| r.tokens.capacity()).sum::<usize>() <= 16_384
        {
            self.0.push_front(Entry {
                patch: Rc::clone(patch),
                path: path.into(),
                dark,
                page: Rc::clone(&page),
            });
            self.0.truncate(8);
        }
        page
    }
}

fn highlight(path: &str, patch: &str, dark: bool, limit: usize) -> Page {
    let ext = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
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
        .zip(crate::comments::coordinates(patch));
    for (line, coord) in lines.by_ref().take(limit) {
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
                rows.push(Row {
                    len: line.len(),
                    class: "hunk",
                    coord: None,
                    tokens: vec![],
                });
                continue;
            }
        };
        let spans = lane
            .as_mut()
            .and_then(|h| h.highlight_line(&line[1..], &SYNTAX).ok());
        let tokens = spans
            .into_iter()
            .flatten()
            .map(|(s, text)| Token {
                len: text.len(),
                rgb: [s.foreground.r, s.foreground.g, s.foreground.b],
            })
            .collect();
        rows.push(Row {
            len: line.len(),
            class,
            coord,
            tokens,
        });
    }
    Page {
        rows,
        more: lines.next().is_some(),
    }
}

#[component]
pub fn Diff(
    path: String,
    patch: Rc<str>,
    dark: bool,
    oncomment: EventHandler<crate::comments::Anchor>,
    anchor: Option<crate::comments::Anchor>,
    guided: Option<(usize, u64)>,
) -> Element {
    let mut limit = use_signal(|| BATCH);
    let visible = limit().max(guided.map_or(0, |(r, _)| r + 1));
    use_effect(use_reactive!(|(guided)| {
        if guided.is_some() {
            document::eval(
                "requestAnimationFrame(() => document.querySelector('.guided-target')?.scrollIntoView({block:'center',inline:'nearest'}))",
            );
        }
    }));
    let cache = use_context::<Rc<RefCell<HighlightCache>>>();
    let page = cache.borrow_mut().page(&path, &patch, dark, visible);
    let mut offset = 0;
    let mut rows = Vec::with_capacity(page.rows.len());
    for (idx, row) in page.rows.iter().enumerate() {
        let line = &patch[offset..offset + row.len];
        offset += row.len;
        let mut tokens = line.get(1..).unwrap_or("");
        let target = anchor
            .as_ref()
            .is_some_and(|a| a.path == path && row.coord == Some((a.side.as_str(), a.line)));
        rows.push(rsx! { div { class:"{row.class}",class:if target {"comment-target"}, class:if guided.is_some_and(|(r,_)|r==idx) {"guided-target"},
            if cfg!(not(target_arch="wasm32")) {
                if let Some((side,n))=row.coord{
                    button {class:"line-comment",disabled:anchor.is_some(),title:format!("Draft a comment · {}",crate::comments::line_label(side,n)),onkeydown:crate::keep_native_space,
                        onclick:{let path=path.clone();move |_|oncomment.call(crate::comments::Anchor{path:path.clone(),side:side.into(),line:n})},"{n} +"}
                }
            }
            if !row.tokens.is_empty() {
                {&line[..1]}
                for token in &row.tokens {
                    {let (text,rest)=tokens.split_at(token.len);tokens=rest;let [r,g,b]=token.rgb;
                    rsx!{span { style:"color:rgb({r},{g},{b})", {text.trim_end_matches('\n')} }}}
                }
            } else { {line.trim_end_matches('\n')} }
        }});
    }
    rsx! { pre { for row in rows { {row} } }
        if page.more{
            button {onclick:move |_|limit.set(visible+BATCH),"Show more lines"}
        }
    }
}
