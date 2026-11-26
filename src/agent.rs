//! Desktop-wide preference. Arguments are literal lines, not shell syntax.
use crate::{gh, platform::store};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::Path;

/// Store the installation source, not a path inside a versioned package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Installed {
    ClaudePath,
    CodexPath,
    ClaudeZed,
    CodexZed,
}

impl Installed {
    const ALL: [Self; 4] = [
        Self::ClaudeZed,
        Self::CodexZed,
        Self::ClaudePath,
        Self::CodexPath,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::ClaudeZed => "Claude Agent — installed by Zed",
            Self::CodexZed => "Codex ACP — installed by Zed",
            Self::ClaudePath => "Claude Agent — system executable",
            Self::CodexPath => "Codex ACP — system executable",
        }
    }
    fn resolve(self) -> Result<OsString, String> {
        let found = match self {
            Self::ClaudePath => gh::resolve("claude-agent-acp").ok(),
            Self::CodexPath => gh::resolve("codex-acp").ok(),
            Self::ClaudeZed | Self::CodexZed => dirs::data_local_dir().and_then(|data| {
                let app = if cfg!(target_os = "macos") || cfg!(windows) {
                    "Zed"
                } else {
                    "zed"
                };
                let registry = data.join(app).join("external_agents/registry");
                let (package, exe) = if self == Self::CodexZed {
                    ("codex-acp", "codex-acp")
                } else {
                    ("claude-acp", "claude-agent-acp")
                };
                zed_package(&registry.join(package), exe).or_else(|| {
                    // Zed also keeps already-installed npm adapters here. Run
                    // the local launcher directly, never npx (which may install).
                    gh::resolve(
                        registry
                            .join("npx")
                            .join(package)
                            .join("node_modules/.bin")
                            .join(exe)
                            .to_str()?,
                    )
                    .ok()
                })
            }),
        };
        found.ok_or_else(|| {
            format!(
                "{} was not found. Choose another executable in Guided review agent settings.",
                self.label()
            )
        })
    }
}

fn zed_package(package: &Path, exe: &str) -> Option<OsString> {
    // Only inspect known package directories; never evaluate registry commands.
    std::fs::read_dir(package)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let version =
                semver::Version::parse(name.strip_prefix("v_")?.split('_').next()?).ok()?;
            let path = entry.path().join(exe);
            Some((version, gh::resolve(path.to_str()?).ok()?))
        })
        .max()
        .map(|(_, path)| path)
}

const KEY: &str = "guided-review-agent";
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub acp: bool,
    executable: String,
    arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    installed: Option<Installed>,
}

impl Agent {
    pub fn load() -> Result<Self, String> {
        store::read(KEY)?
            .map(|s| {
                serde_json::from_str(&s).map_err(|e| format!("Invalid guided agent settings: {e}"))
            })
            .unwrap_or(Ok(Self::default()))
    }
    pub fn name(&self) -> &str {
        if !self.acp {
            "Codex"
        } else if let Some(source) = self.installed {
            source.label()
        } else {
            &self.executable
        }
    }
    pub fn resolve(&self) -> Result<OsString, String> {
        if !self.acp {
            return gh::resolve("codex");
        }
        if let Some(source) = self.installed {
            return source.resolve();
        }
        if self.executable.is_empty() {
            return Err("Choose an ACP executable in Ferriscope settings.".into());
        }
        gh::resolve(&self.executable)
    }
    pub fn args(&self) -> impl Iterator<Item = &str> {
        self.arguments.lines()
    }
}

#[component]
pub fn Settings() -> Element {
    let mut selected = use_context::<Signal<Result<Agent, String>>>();
    let mut draft = use_signal(|| selected().unwrap_or_default());
    let mut status = use_signal(String::new);
    let discover = move || {
        Installed::ALL
            .into_iter()
            .filter(|s| s.resolve().is_ok())
            .collect::<Vec<_>>()
    };
    let enabled = use_memo(move || draft.read().acp);
    let choices = use_memo(move || if enabled() { discover() } else { Vec::new() });
    let source = draft.read().installed;
    rsx! {fieldset {
        legend {"Guided review agent"}
        for (acp, label) in [(false,"Local Codex CLI"),(true,"Local ACP agent")] {
            label {input {r#type:"radio",name:"guided-agent",checked:draft.read().acp==acp,onchange:move |_|{
                let mut d=draft.write();
                d.acp=acp;
                if acp && d.installed.is_none() && d.executable.is_empty() && d.arguments.is_empty(){
                    d.installed=discover().first().copied();
                }
                status.set(String::new());
            }} "{label}"}
        }
        if draft.read().acp {
            if !choices.read().is_empty() || source.is_some() {
                label {"Installed agent" select {
                    value:source.map(|s|s.label()).unwrap_or("manual"),
                    onchange:move|e|{
                        let next=Installed::ALL.into_iter().find(|s|s.label()==e.value());
                        if draft.read().installed!=next {
                            let mut d=draft.write(); d.installed=next; d.arguments.clear();
                        }
                        status.set(String::new());
                    },
                    option {value:"manual","Custom executable…"}
                    for candidate in Installed::ALL.into_iter().filter(|s|choices.read().contains(s) || Some(*s)==source){
                        option {value:candidate.label(),"{candidate.label()}" if !choices.read().contains(&candidate) {" (not found)"}}
                    }
                }}
            } else {small {"No recognized local adapter found. Choose an executable below."}}
            if source.is_none() {
                label {"Executable (name or absolute path)" input {value:"{draft.read().executable}",oninput:move |e|draft.write().executable=e.value()}}
                button {onclick:move |_|async move {
                    if let Some(file)=rfd::AsyncFileDialog::new().set_title("Choose an ACP executable").pick_file().await {
                        if let Some(path)=file.path().to_str(){draft.write().executable=path.to_owned();status.set(String::new());}
                        else {status.set("Choose an executable with a UTF-8 path.".into());}
                    }
                },"Browse for executable…"}
            }
            details {summary {"Advanced"} label {"Arguments (one per line, no quotes)" textarea {value:"{draft.read().arguments}",oninput:move |e|draft.write().arguments=e.value()}}}
        }
        button {onclick:move |_|{
            let value=draft();
            if value.acp && let Err(e)=value.resolve(){status.set(e);return;}
            match serde_json::to_string(&value).map_err(|e|e.to_string()).and_then(|s|store::write(KEY,&s)) {
                Ok(())=>{selected.set(Ok(value));status.set("Saved.".into());}, Err(e)=>status.set(e)
            }
        },"Save agent"}
        span {role:"status","{status}"}
    }}
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn zed_updates_follow_the_newest_executable_in_the_selected_package() {
        let root = tempfile::tempdir().unwrap();
        let install = |name: &str, mode| {
            let dir = root.path().join(name);
            fs::create_dir(&dir).unwrap();
            let path = dir.join("claude-agent-acp");
            fs::write(&path, "not a runnable program").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            path.into_os_string()
        };
        let old = install("v_0.9.9_hash", 0o755);
        assert_eq!(
            zed_package(root.path(), "claude-agent-acp"),
            Some(old.clone())
        );
        let new = install("v_0.10.0_hash", 0o755);
        install("v_0.11.0_incomplete", 0o644);
        install("unexpected", 0o755);
        assert_eq!(
            zed_package(root.path(), "claude-agent-acp"),
            Some(new.clone())
        );
        fs::remove_file(new).unwrap();
        fs::remove_file(old).unwrap();
        assert_eq!(zed_package(root.path(), "claude-agent-acp"), None);
    }

    #[test]
    fn existing_manual_settings_are_not_reinterpreted_as_discovered() {
        let agent: Agent =
            serde_json::from_str(r#"{"acp":true,"executable":"/my agent","arguments":"one arg"}"#)
                .unwrap();
        assert_eq!(agent.installed, None);
        assert_eq!(agent.name(), "/my agent");
        assert_eq!(agent.args().collect::<Vec<_>>(), ["one arg"]);
    }
}
