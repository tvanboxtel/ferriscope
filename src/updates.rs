use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
#[component]
pub fn UpdateCheck() -> Element {
    rsx! {}
}

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::UpdateCheck;

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use super::*;
    use semver::Version;

    const CURRENT: &str = env!("CARGO_PKG_VERSION");
    const RELEASES: &str = "https://github.com/tvanboxtel/ferriscope/releases";

    #[derive(Clone, PartialEq, Debug)]
    enum Check {
        Idle,
        Checking,
        Current,
        Available(String),
        Failed(String),
    }

    fn compare(current: &str, tag: &str) -> Result<Check, String> {
        let version = Version::parse(tag.strip_prefix('v').unwrap_or(tag))
            .map_err(|_| "GitHub returned an unrecognized release version.".to_string())?;
        let installed = Version::parse(current).expect("Cargo package version is semver");
        Ok(if version.cmp_precedence(&installed).is_gt() {
            Check::Available(tag.to_owned())
        } else {
            Check::Current
        })
    }

    async fn check() -> Result<Check, String> {
        let output = async_process::Command::new(crate::gh::executable()?)
            .args([
                "api",
                "--hostname",
                "github.com",
                "repos/tvanboxtel/ferriscope/releases/latest",
                "--jq",
                ".tag_name",
            ])
            .output()
            .await
            .map_err(|_| "Could not start GitHub CLI. Is gh installed?".to_string())?;
        if !output.status.success() {
            return Err(
                "Could not check GitHub Releases. Check your connection and gh login, then retry."
                    .into(),
            );
        }
        compare(CURRENT, String::from_utf8_lossy(&output.stdout).trim())
    }

    #[component]
    pub fn UpdateCheck() -> Element {
        let mut state = use_signal(|| Check::Idle);
        let checking = matches!(*state.read(), Check::Checking);
        rsx! {
            button { class:"update-check", disabled:checking, title:"Installed version: {CURRENT}",
                onclick:move |_| {
                    if matches!(*state.peek(), Check::Checking) { return; }
                    state.set(Check::Checking);
                    spawn(async move { state.set(check().await.unwrap_or_else(Check::Failed)); });
                },
                if checking {"Checking…"} else {"Check for updates"}
            }
            span { role:"status",
                match state() {
                    Check::Idle | Check::Checking => rsx! {},
                    Check::Current => rsx! {"Up to date (v{CURRENT})."},
                    Check::Available(tag) => rsx! {"{tag} available. " a {href:"{RELEASES}/tag/{tag}", target:"_blank", rel:"noopener noreferrer", "View release ↗"}},
                    Check::Failed(msg) => rsx! {"{msg} " a {href:RELEASES,target:"_blank",rel:"noopener noreferrer","View releases ↗"}},
                }
            }
        }
    }

    #[test]
    fn compares_release_precedence_not_tag_text() {
        assert_eq!(
            compare("0.9.0", "v0.10.0"),
            Ok(Check::Available("v0.10.0".into()))
        );
        assert_eq!(compare("1.0.0", "1.0.0+build"), Ok(Check::Current));
        assert_eq!(compare("1.1.0", "v1.0.0"), Ok(Check::Current));
        assert!(compare("1.0.0", "nightly").is_err());
    }
}
