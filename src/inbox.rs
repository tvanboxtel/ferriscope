use dioxus::prelude::*;
use ferriscope::pr_key;
use serde::Deserialize;
use std::{collections::HashMap, ffi::OsStr, time::Duration};

const PAGE_SIZE: usize = 30;

#[derive(Deserialize)]
struct SearchPage {
    total_count: usize,
    incomplete_results: bool,
    items: Vec<SearchPr>,
}

impl SearchPage {
    fn is_partial(&self) -> bool {
        self.incomplete_results || self.total_count > self.items.len()
    }
}

#[derive(Deserialize)]
struct SearchPr {
    html_url: String,
    title: String,
    draft: bool,
}

struct InboxEntry {
    key: String,
    title: String,
    draft: bool,
    review_requested: bool,
    assigned: bool,
}

impl InboxEntry {
    fn reference(&self) -> String {
        let (repo, number) = self.key.rsplit_once("/pull/").expect("canonical PR key");
        format!("{repo} #{number}")
    }
}

struct Inbox {
    entries: Vec<InboxEntry>,
    partial: bool,
}

impl Inbox {
    fn merge(requested: SearchPage, assigned: SearchPage) -> Self {
        let partial = requested.is_partial() || assigned.is_partial();
        let mut entries: Vec<InboxEntry> = Vec::new();
        let mut positions = HashMap::<String, usize>::new();
        // GitHub sorts each page by last update. Keep that order within each
        // group, placing requests ahead of assignments and showing overlaps once.
        for (page, review_requested) in [(requested, true), (assigned, false)] {
            for pr in page.items {
                let Ok(key) = pr_key(&pr.html_url) else {
                    continue;
                };
                if let Some(&index) = positions.get(&key) {
                    entries[index].assigned |= !review_requested;
                } else {
                    positions.insert(key.clone(), entries.len());
                    entries.push(InboxEntry {
                        key,
                        title: pr.title,
                        draft: pr.draft,
                        review_requested,
                        assigned: !review_requested,
                    });
                }
            }
        }
        Self { entries, partial }
    }
}

async fn search(gh: &OsStr, qualifier: &str) -> Result<SearchPage, String> {
    let output = async_process::Command::new(gh)
        .args([
            "api",
            "--hostname",
            "github.com",
            "--method",
            "GET",
            "search/issues",
            "-f",
            &format!("q=is:pr is:open {qualifier}"),
            "-f",
            "sort=updated",
            "-f",
            "order=desc",
            "-F",
            &format!("per_page={PAGE_SIZE}"),
        ])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("Could not run GitHub CLI (gh): {e}"))?;
    if !output.status.success() {
        return Err("Check your connection and GitHub login (`gh auth status --hostname github.com`), then refresh.".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "GitHub returned an unreadable PR list. Try refreshing.".into())
}

async fn load() -> Result<Inbox, String> {
    let gh = crate::gh::executable()?;
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        futures_util::try_join!(
            search(&gh, "review-requested:@me"),
            search(&gh, "assignee:@me")
        )
    })
    .await
    .map_err(|_| "GitHub took too long to respond. Try refreshing.".to_string())?;
    let (requested, assigned) = result?;
    Ok(Inbox::merge(requested, assigned))
}

#[component]
pub fn YourPrs(open: Callback<String>, busy: bool) -> Element {
    let mut inbox = use_resource(load);
    let loading = inbox.state()() == UseResourceState::Pending;
    let result = inbox.read();
    rsx! {
        section {class:"inbox",aria_labelledby:"inbox-title",
            div {class:"inbox-heading",
                h1 {id:"inbox-title","Your PRs"}
                button {disabled:loading,onclick:move |_|inbox.restart(),"Refresh inbox"}
            }
            p {"Open pull requests waiting for your review or assigned to you."}
            if loading { p {role:"status","Loading your PRs…"} }
            match result.as_ref() {
                Some(Ok(inbox)) => rsx! {
                    if inbox.partial {
                        p {role:"status","GitHub returned only part of your inbox. This view shows up to {PAGE_SIZE} PRs per category."}
                    }
                    if inbox.entries.is_empty() && !inbox.partial && !loading {
                        p {role:"status","No open PRs are waiting for your review or assigned to you."}
                    }
                    if !inbox.entries.is_empty() {
                        ul {class:"inbox-list",
                            for entry in &inbox.entries {
                                li {key:"{entry.key}",
                                    button {
                                        disabled:busy,
                                        onclick:{
                                            let url=format!("https://github.com/{}",entry.key);
                                            move |_| open.call(url.clone())
                                        },
                                        strong {"{entry.title}"}
                                        span {class:"inbox-meta",
                                            span { {entry.reference()} }
                                            if entry.review_requested { span {class:"inbox-badge requested","Review requested"} }
                                            if entry.assigned { span {class:"inbox-badge","Assigned to you"} }
                                            if entry.draft { span {class:"inbox-badge","Draft"} }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(error)) if !loading => rsx! {
                    p {role:"alert","Couldn’t load your PRs. {error}"}
                },
                _ => rsx! {},
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(paths: &[&str]) -> SearchPage {
        SearchPage {
            total_count: paths.len(),
            incomplete_results: false,
            items: paths
                .iter()
                .map(|path| SearchPr {
                    html_url: format!("https://github.com/{path}"),
                    title: format!("PR {path}"),
                    draft: false,
                })
                .collect(),
        }
    }

    #[test]
    fn prioritizes_review_requests_and_combines_labels_without_reordering() {
        let inbox = Inbox::merge(
            page(&["owner/repo/pull/1", "owner/repo/pull/2"]),
            page(&[
                "owner/repo/pull/3",
                "Owner/Repo/pull/2",
                "other/repo/pull/2",
            ]),
        );
        let rows: Vec<_> = inbox
            .entries
            .iter()
            .map(|e| (e.key.as_str(), e.review_requested, e.assigned))
            .collect();
        assert_eq!(
            rows,
            [
                ("owner/repo/pull/1", true, false),
                ("owner/repo/pull/2", true, true),
                ("owner/repo/pull/3", false, true),
                ("other/repo/pull/2", false, true),
            ]
        );
        assert!(!inbox.partial);
    }

    #[test]
    fn distinguishes_a_partial_inbox_from_an_empty_one() {
        let empty = Inbox::merge(page(&[]), page(&[]));
        assert!(empty.entries.is_empty());
        assert!(!empty.partial);

        let mut requested = page(&[]);
        requested.incomplete_results = true;
        assert!(Inbox::merge(requested, page(&[])).partial);

        let mut assigned = page(&["owner/repo/pull/1"]);
        assigned.total_count = 31;
        let limited = Inbox::merge(page(&[]), assigned);
        assert_eq!(limited.entries.len(), 1);
        assert!(limited.partial);
    }
}
