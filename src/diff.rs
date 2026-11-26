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

// Emit one highlighted fragment per line across the desktop WebView bridge.
// Only numeric colors become attributes; patch contents are always escaped.
fn colored_line<'a>(tokens: impl IntoIterator<Item = ([u8; 3], &'a str)>) -> String {
    use std::fmt::Write;
    let mut html = String::new();
    let mut previous = None;
    for (rgb, text) in tokens {
        let text = text.trim_end_matches('\n');
        if text.is_empty() {
            continue;
        }
        if previous != Some(rgb) {
            if previous.is_some() {
                html.push_str("</span>")
            }
            let [r, g, b] = rgb;
            write!(html, "<span style=\"color:#{r:02x}{g:02x}{b:02x}\">").unwrap();
            previous = Some(rgb);
        }
        for ch in text.chars() {
            match ch {
                '<' => html.push_str("&lt;"),
                '>' => html.push_str("&gt;"),
                '&' => html.push_str("&amp;"),
                _ => html.push(ch),
            }
        }
    }
    if previous.is_some() {
        html.push_str("</span>")
    }
    html
}
#[cfg(test)]
#[test]
fn patch_markup_remains_text() {
    let text = "</span><img src=x>&amp;\n";
    assert_eq!(
        colored_line([([0, 1, 2], text)]),
        "<span style=\"color:#000102\">&lt;/span&gt;&lt;img src=x&gt;&amp;amp;</span>"
    );
}
struct Row {
    len: usize,
    class: &'static str,
    coord: Option<(&'static str, u32)>,
    markup: String,
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
    fn cached(
        &mut self,
        path: &str,
        patch: &Rc<str>,
        dark: bool,
        limit: usize,
    ) -> Option<Rc<Page>> {
        if limit == BATCH
            && let Some(i) = self
                .0
                .iter()
                .position(|e| e.dark == dark && e.path == path && Rc::ptr_eq(&e.patch, patch))
        {
            let hit = self.0.remove(i).unwrap();
            let page = Rc::clone(&hit.page);
            self.0.push_front(hit);
            return Some(page);
        }
        None
    }
    fn remember(&mut self, path: &str, patch: &Rc<str>, dark: bool, limit: usize, page: Rc<Page>) {
        // At most eight first pages and 1 MiB of colored markup per page.
        if limit == BATCH
            && page.rows.iter().map(|r| r.markup.capacity()).sum::<usize>() <= 1024 * 1024
        {
            self.0.push_front(Entry {
                patch: Rc::clone(patch),
                path: path.into(),
                dark,
                page: Rc::clone(&page),
            });
            self.0.truncate(8);
        }
    }
}

fn highlight(path: &str, patch: &str, dark: bool, limit: usize, color: bool) -> Page {
    let ext = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit('.')
        .next()
        .unwrap_or("");
    let syntax = color
        .then(|| SYNTAX.find_syntax_by_extension(ext))
        .flatten();
    let fresh = || syntax.map(|s| HighlightLines::new(s, theme(dark)));
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
                    markup: String::new(),
                });
                continue;
            }
        };
        let spans = lane
            .as_mut()
            .and_then(|h| h.highlight_line(&line[1..], &SYNTAX).ok());
        let markup = colored_line(
            spans
                .into_iter()
                .flatten()
                .map(|(s, text)| ([s.foreground.r, s.foreground.g, s.foreground.b], text)),
        );
        rows.push(Row {
            len: line.len(),
            class,
            coord,
            markup,
        });
    }
    Page {
        rows,
        more: lines.next().is_some(),
    }
}

#[cfg(test)]
#[test]
fn cached_markup_only_matches_the_same_patch_path_and_theme() {
    let patch: Rc<str> = Rc::from("@@ -0,0 +1 @@\n+const answer = 42;\n");
    let page = Rc::new(highlight("index.ts", &patch, false, BATCH, true));
    assert!(page.rows[1].markup.contains("<span"));
    let mut cache = HighlightCache::default();
    cache.remember("index.ts", &patch, false, BATCH, page.clone());
    assert!(Rc::ptr_eq(
        &cache.cached("index.ts", &patch, false, BATCH).unwrap(),
        &page
    ));
    assert!(cache.cached("index.ts", &patch, true, BATCH).is_none());
    assert!(cache.cached("index.js", &patch, false, BATCH).is_none());
    assert!(
        cache
            .cached("index.ts", &Rc::from("+new"), false, BATCH)
            .is_none()
    );
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
    #[cfg(not(target_arch = "wasm32"))]
    let page = {
        let worker_cache = cache.clone();
        let filename = path.clone();
        let data = patch.clone();
        let colored = use_resource(use_reactive!(|(filename, data, dark, visible)| {
            let (path, patch) = (filename, data);
            let cache = worker_cache.clone();
            let count = visible;
            async move {
                let hit = cache.borrow_mut().cached(&path, &patch, dark, count);
                let page = if let Some(hit) = hit {
                    hit
                } else {
                    // Only send the visible batch and one lookahead line to the worker.
                    let end = patch
                        .match_indices('\n')
                        .nth(count)
                        .map_or(patch.len(), |(i, _)| i + 1);
                    let sample = patch[..end].to_owned();
                    let filename = path.clone();
                    let page = Rc::new(
                        tokio::task::spawn_blocking(move || {
                            highlight(&filename, &sample, dark, count, true)
                        })
                        .await
                        .ok()?,
                    );
                    cache
                        .borrow_mut()
                        .remember(&path, &patch, dark, count, page.clone());
                    page
                };
                Some((path, patch, dark, count, page))
            }
        }));
        let ready = colored.read();
        ready
            .as_ref()
            .and_then(Option::as_ref)
            .filter(|(p, data, d, n, _)| {
                p == &path && Rc::ptr_eq(data, &patch) && *d == dark && *n == visible
            })
            .map(|(_, _, _, _, page)| page.clone())
            .or_else(|| cache.borrow_mut().cached(&path, &patch, dark, visible))
            .unwrap_or_else(|| Rc::new(highlight(&path, &patch, dark, visible, false)))
    };
    #[cfg(target_arch = "wasm32")]
    let page = {
        let mut cache = cache.borrow_mut();
        cache
            .cached(&path, &patch, dark, visible)
            .unwrap_or_else(|| {
                let page = Rc::new(highlight(&path, &patch, dark, visible, true));
                cache.remember(&path, &patch, dark, visible, page.clone());
                page
            })
    };
    let mut offset = 0;
    let mut rows = Vec::with_capacity(page.rows.len());
    for (idx, row) in page.rows.iter().enumerate() {
        let line = &patch[offset..offset + row.len];
        offset += row.len;
        let (prefix, content) = line.split_at(usize::from(line.starts_with(['+', '-', ' '])));
        let new_line = row
            .coord
            .and_then(|(side, n)| (side == "RIGHT").then_some(n.to_string()));
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
            {prefix}
            if !row.markup.is_empty() {
                span {"data-definition-line":new_line.clone(),dangerous_inner_html:row.markup.as_str()}
            } else { span {"data-definition-line":new_line,{content.trim_end_matches('\n')}} }
        }});
    }
    rsx! { pre { for row in rows { {row} } }
        if page.more{
            button {onclick:move |_|limit.set(visible+BATCH),"Show more lines"}
        }
    }
}
