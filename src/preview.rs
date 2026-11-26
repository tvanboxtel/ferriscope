use crate::diff::{SYNTAX, theme};
use dioxus::prelude::*;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use std::{fmt::Write, time::Duration};
use syntect::easy::HighlightLines;

#[component]
pub fn Editor(
    label: String,
    mut body: Signal<String>,
    dark: bool,
    #[props(default)] focus: bool,
    #[props(default)] disabled: bool,
    #[props(default)] placeholder: String,
) -> Element {
    // Let WebKit own the caret, selection and IME composition. Only seed its
    // value on mount (e.g. returning from an inline note to a pending summary).
    let initial = use_hook(|| body.peek().clone());
    let mut live = use_signal(|| true);
    rsx! {
        label { "{label}"
            textarea {initial_value:initial,disabled,placeholder,
                onmounted:move |e|async move{if focus {let _=e.set_focus(true).await;}},
                oninput:move |e|body.set(e.value())
            }
        }
        label {class:"preview-toggle",input {r#type:"checkbox",checked:live(),onchange:move |_|live.toggle()} " Live preview"}
        if live(){ Preview {body,dark} }
    }
}

#[component]
fn Preview(body: Signal<String>, dark: bool) -> Element {
    // A new edit cancels the previous timer. Only take a snapshot when typing
    // pauses. In-flight blocking work cannot overwrite a newer resource.
    let result = use_resource(use_reactive!(|(dark)| async move {
        body.read();
        tokio::time::sleep(Duration::from_millis(180)).await;
        let source = body.peek().clone();
        if source.trim().is_empty() {
            return None;
        }
        tokio::task::spawn_blocking(move || render(&source, dark))
            .await
            .ok()
    }));
    rsx! {
        div {class:"comment-preview",aria_label:"Markdown preview",
            if let Some(Some(markup))=&*result.read(){
                div {dangerous_inner_html:markup.as_str()}
            }else{small {"Markdown preview · use fenced code blocks for highlighting"}}
        }
    }
}

fn render(source: &str, dark: bool) -> String {
    let mut parser = Parser::new_ext(
        source,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES,
    );
    let mut events = Vec::new();
    while let Some(event) = parser.next() {
        match event {
            // Never execute author HTML or navigate/load remote resources from
            // the privileged desktop webview. Keep link text and image alt text.
            Event::Html(s) | Event::InlineHtml(s) => events.push(Event::Text(s)),
            Event::Start(Tag::Link { .. } | Tag::Image { .. })
            | Event::End(TagEnd::Link | TagEnd::Image) => {}
            Event::Start(Tag::CodeBlock(kind)) => {
                let mut code = String::new();
                for token in parser.by_ref() {
                    match token {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(s) => code.push_str(&s),
                        _ => {}
                    }
                }
                let lang = match &kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next(),
                    _ => None,
                };
                let mut highlighter = lang
                    .filter(|_| code.len() <= 32_768)
                    .and_then(|l| SYNTAX.find_syntax_by_token(l))
                    .map(|s| HighlightLines::new(s, theme(dark)));
                let mut out = String::from("<pre><code>");
                for line in code.split_inclusive('\n') {
                    if let Some(tokens) = highlighter
                        .as_mut()
                        .and_then(|h| h.highlight_line(line, &SYNTAX).ok())
                    {
                        for (style, s) in tokens {
                            let c = style.foreground;
                            write!(
                                out,
                                "<span style=\"color:#{:02x}{:02x}{:02x}\">",
                                c.r, c.g, c.b
                            )
                            .unwrap();
                            escape_text(&mut out, s);
                            out.push_str("</span>");
                        }
                    } else {
                        escape_text(&mut out, line);
                    }
                }
                out.push_str("</code></pre>");
                events.push(Event::Html(out.into()));
            }
            other => events.push(other),
        }
    }
    let mut out = String::new();
    html::push_html(&mut out, events.into_iter());
    out
}

fn escape_text(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "performance measurement; run in release mode"]
    fn measure_fenced_preview() {
        let source = format!(
            "Suggested fix:\n```rust\n{}\n```",
            "let x = Some(42);\n".repeat(100)
        );
        for run in 1..=5 {
            let start = std::time::Instant::now();
            let html = render(&source, true);
            let elapsed = start.elapsed();
            assert!(html.contains("<span style="));
            println!("100-line Markdown code preview render #{run}: {elapsed:?}");
        }
    }
    #[test]
    fn preview_formats_markdown_and_code_without_active_content() {
        let input = "**Review** `x`\n\n![image](https://example.org/img) [link](javascript:alert)\n\n<img src=x onerror=alert(1)>\n\n```rust\nlet x = \"</code><script>\";\n```";
        let light = render(input, false);
        for expected in [
            "<strong>Review</strong>",
            "<code>x</code>",
            "<span style=",
            "&lt;",
        ] {
            assert!(light.contains(expected));
        }
        for forbidden in ["<img", "<script", "href=", "src=\"https"] {
            assert!(!light.contains(forbidden));
        }
        assert_ne!(light, render(input, true));
        assert!(
            render("```unknown-lang\nx < y && z\n```", false).contains("x &lt; y &amp;&amp; z")
        );
    }
}
