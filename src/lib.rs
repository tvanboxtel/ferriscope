use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

pub fn pr_key(input: &str) -> Result<String, String> {
    let path = input.trim().split(['?', '#']).next().unwrap_or("");
    let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
    if let [
        "https:",
        "",
        "github.com",
        owner,
        repo,
        "pull",
        n,
        rest @ ..,
    ] = parts.as_slice()
        && (rest.is_empty() || rest == ["files"])
        && [owner, repo].iter().all(|s| {
            !s.is_empty()
                && **s != "."
                && **s != ".."
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
        && let Ok(number @ 1..) = n.parse::<u64>()
    {
        return Ok(format!(
            "{}/{}/pull/{number}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        ));
    }
    Err("Paste an https://github.com/owner/repo/pull/123 URL.".into())
}

pub const KINDS: [&str; 6] = [
    "Implementation",
    "Tests",
    "Infra",
    "Docs",
    "Generated/lockfiles",
    "Other",
];
pub fn kind(path: &str) -> &'static str {
    let p = path.to_ascii_lowercase();
    let parts: Vec<_> = p.split('/').collect();
    let name = parts.last().unwrap();
    let ext = name.rsplit('.').next().unwrap();
    let has = |names: &[&str]| parts.iter().any(|s| names.contains(s));
    let i = if name.ends_with(".lock")
        || matches!(*name, "package-lock.json" | "pnpm-lock.yaml" | "go.sum")
        || name.contains(".generated.")
        || has(&["generated", "dist"])
    {
        4
    } else if has(&["test", "tests", "__tests__", "spec"])
        || name.starts_with("test_")
        || name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
    {
        1
    } else if has(&["docs"]) || matches!(ext, "md" | "rst") || name.starts_with("readme") {
        3
    } else if has(&[".github", "infra", "terraform", "deploy", "migrations"])
        || matches!(ext, "tf" | "tfvars")
        || matches!(*name, "dockerfile" | "makefile")
    {
        2
    } else if matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "go"
            | "c"
            | "cpp"
            | "h"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "sql"
            | "css"
            | "html"
            | "vue"
            | "cs"
            | "php"
    ) {
        0
    } else {
        5
    };
    KINDS[i]
}

#[derive(Clone, Deserialize, PartialEq)]
pub struct File {
    pub filename: String,
    sha: String,
    status: String,
    patch: Option<std::rc::Rc<str>>,
    previous_filename: Option<String>,
    additions: Option<u32>,
    deletions: Option<u32>,
    #[serde(skip)]
    fingerprint: std::cell::OnceCell<String>,
}
pub fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
impl File {
    pub fn change_type(&self) -> &str {
        &self.status
    }
    pub fn patch(&self) -> Option<&std::rc::Rc<str>> {
        self.patch.as_ref()
    }
    /// Use GitHub's totals even when it omits or truncates the text patch.
    pub fn line_changes(&self) -> Option<(u32, u32)> {
        self.additions.zip(self.deletions)
    }
    pub fn fingerprint(&self) -> &str {
        self.fingerprint.get_or_init(|| {
            hash(
                &serde_json::to_string(&(
                    &self.status,
                    &self.previous_filename,
                    &self.patch,
                    self.patch.is_none().then_some(&self.sha),
                ))
                .unwrap(),
            )
        })
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub name: String,
    pub custom: bool,
    pub files: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Review {
    pub groups: Vec<Group>,
    reviewed: BTreeMap<String, String>,
}
impl Default for Review {
    fn default() -> Self {
        Self {
            groups: KINDS
                .map(|name| Group {
                    name: name.into(),
                    custom: false,
                    files: vec![],
                })
                .into(),
            reviewed: BTreeMap::new(),
        }
    }
}
impl Review {
    pub fn reconcile(&mut self, files: &[File]) {
        let live: HashSet<_> = files.iter().map(|f| f.filename.as_str()).collect();
        let mut seen = HashSet::new();
        for g in &mut self.groups {
            g.files
                .retain(|p| live.contains(p.as_str()) && seen.insert(p.clone()));
        }
        self.reviewed.retain(|p, _| live.contains(p.as_str()));
        for f in files {
            if seen.insert(f.filename.clone()) {
                if let Some(g) = self
                    .groups
                    .iter_mut()
                    .find(|g| !g.custom && g.name == kind(&f.filename))
                {
                    g.files.push(f.filename.clone());
                }
            }
        }
    }
    pub fn together(&mut self, name: &str, selected: &HashSet<String>) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty()
            || self
                .groups
                .iter()
                .any(|g| g.name.eq_ignore_ascii_case(name))
        {
            return Err("Choose a unique, nonempty group name.".into());
        }
        let files: Vec<_> = self
            .groups
            .iter()
            .flat_map(|g| g.files.iter())
            .filter(|p| selected.contains(*p))
            .cloned()
            .collect();
        if files.len() < 2 {
            return Err("Select at least two files.".into());
        }
        for g in &mut self.groups {
            g.files.retain(|p| !selected.contains(p));
        }
        self.groups.insert(
            0,
            Group {
                name: name.into(),
                custom: true,
                files,
            },
        );
        Ok(())
    }
    pub fn remove_group(&mut self, name: &str, files: &[File]) -> Result<(), String> {
        let index = self
            .groups
            .iter()
            .position(|g| g.custom && g.name == name)
            .ok_or("Only custom groups can be removed.")?;
        self.groups.remove(index);
        self.reconcile(files);
        Ok(())
    }
    pub fn status(&self, f: &File) -> &'static str {
        match self.reviewed.get(&f.filename) {
            None => "Unread",
            Some(h) if h == f.fingerprint() => "Reviewed",
            _ => "Changed",
        }
    }
    pub fn toggle(&mut self, f: &File) {
        if self.status(f) == "Reviewed" {
            self.reviewed.remove(&f.filename);
        } else {
            self.reviewed
                .insert(f.filename.clone(), f.fingerprint().into());
        }
    }
}
