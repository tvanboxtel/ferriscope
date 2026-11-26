use ferriscope::{File, Review};
use futures_util::{StreamExt, TryStreamExt, stream};
use serde::Deserialize;

#[cfg(target_arch = "wasm32")]
async fn request(path: &str) -> Result<String, String> {
    let r = gloo_net::http::Request::get(&format!("https://api.github.com/{path}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.ok() {
        return Err(format!(
            "GitHub returned HTTP {}. Browser access requires a public PR and remaining API quota.",
            r.status()
        ));
    }
    r.text().await.map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
async fn request(path: &str) -> Result<String, String> {
    let r = async_process::Command::new("gh")
        .args(["api", "--hostname", "github.com", path])
        .output()
        .await
        .map_err(|e| format!("Cannot run gh: {e}"))?;
    if !r.status.success() {
        return Err(String::from_utf8_lossy(&r.stderr).into_owned());
    }
    String::from_utf8(r.stdout).map_err(|e| e.to_string())
}

#[derive(Deserialize, PartialEq)]
struct Ref {
    sha: String,
}
#[derive(Deserialize, PartialEq)]
struct Meta {
    head: Ref,
    base: Ref,
    changed_files: usize,
}
pub async fn fetch(key: &str) -> Result<(String, Vec<File>), String> {
    let api = format!("repos/{}", key.replace("/pull/", "/pulls/"));
    let parse = |s: &str| serde_json::from_str::<Meta>(s).map_err(|e| e.to_string());
    let meta = parse(&request(&api).await?)?;
    if meta.changed_files > 3000 {
        return Err("GitHub's files API is limited to 3,000 files per PR.".into());
    }
    // Keep GitHub order while overlapping latency; never fan out to all 30 pages.
    let pages = stream::iter(1..=meta.changed_files.div_ceil(100))
        .map(|page| {
            let path = format!("{api}/files?per_page=100&page={page}");
            async move {
                serde_json::from_str::<Vec<File>>(&request(&path).await?).map_err(|e| e.to_string())
            }
        })
        .buffered(4);
    let files = pages
        .try_fold(
            Vec::with_capacity(meta.changed_files),
            |mut files, page| async move {
                files.extend(page);
                Ok(files)
            },
        )
        .await?;
    if files.len() != meta.changed_files || parse(&request(&api).await?)? != meta {
        return Err("The PR changed while loading. Try refreshing.".into());
    }
    Ok((meta.head.sha, files))
}

#[cfg(target_arch = "wasm32")]
mod store {
    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .ok_or("Browser storage unavailable".into())
    }
    pub fn read(k: &str) -> Result<Option<String>, String> {
        storage()?
            .get_item(&format!("ferriscope:v1:{k}"))
            .map_err(|_| "Cannot read browser storage".into())
    }
    pub fn write(k: &str, v: &str) -> Result<(), String> {
        storage()?
            .set_item(&format!("ferriscope:v1:{k}"), v)
            .map_err(|_| "Cannot save browser progress (storage full or disabled)".into())
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod store {
    use std::{fs, io::Write, path::PathBuf};
    fn path(k: &str) -> Result<PathBuf, String> {
        Ok(dirs::data_local_dir()
            .ok_or("No application data directory")?
            .join("ferriscope")
            .join(format!("{}.json", ferriscope::hash(k))))
    }
    pub fn read(k: &str) -> Result<Option<String>, String> {
        match fs::read_to_string(path(k)?) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn write(k: &str, v: &str) -> Result<(), String> {
        let path = path(k)?;
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let mut f = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        f.write_all(v.as_bytes()).map_err(|e| e.to_string())?;
        f.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn load(key: &str) -> Result<Review, String> {
    Ok(store::read(key)?
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default())
}
pub fn load_theme() -> String {
    store::read("appearance")
        .ok()
        .flatten()
        .filter(|s| matches!(s.as_str(), "light" | "dark" | "dusk"))
        .unwrap_or_else(|| "light".into())
}
pub fn save_theme(theme: &str) -> Result<(), String> {
    store::write("appearance", theme)
}
pub fn save(key: &str, state: &Review) -> Result<(), String> {
    store::write(
        key,
        &serde_json::to_string(state).map_err(|e| e.to_string())?,
    )
}
