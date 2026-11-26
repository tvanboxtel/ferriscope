use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

const PR: &str = "demo/service/pull/1";
fn executable(dir: &Path, script: &str) -> std::path::PathBuf {
    let exe = dir.join("fake-codex");
    fs::write(&exe, format!("#!/bin/sh\nset -eu\n{script}")).unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    exe
}
#[test]
fn generates_and_validates_without_ever_running_real_codex() {
    let dir = tempfile::tempdir().unwrap();
    let head = "fixture-revision".to_owned();
    fs::write(dir.path().join("unchanged.ts"), "export const tax = 20;").unwrap();
    let root = dir.path().canonicalize().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let exe = executable(
        tools.path(),
        r#"
case " $* " in *" --sandbox read-only "*) ;; *) exit 11;; esac
while [ "$1" != --output-last-message ]; do shift; done
output=$2
grep -F 'Explain the fallback behavior' >/dev/null
test "$PWD" = "$FERRISCOPE_PR_BUNDLE"
grep -F tax "$FERRISCOPE_PR_SOURCE/unchanged.ts" >/dev/null
head=fixture-revision
cat > "$output" <<EOF
{"pr":"demo/service/pull/1","head":"$head","stops":[
{"path":"service.ts","side":"RIGHT","line":10,"why":"Follow entry."},
{"path":"service.ts","side":"LEFT","line":7,"why":"Compare fallback."},
{"path":"test.ts","side":"RIGHT","line":2,"why":"Check behavior."},
{"path":"test.ts","side":"LEFT","line":2,"why":"Unavailable."}]}
EOF"#,
    );
    let files:Vec<File>=serde_json::from_value(serde_json::json!([
        {"filename":"service.ts","sha":"1","status":"modified","patch":"@@ -7 +10 @@\n-before\n+after"},
        {"filename":"test.ts","sha":"2","status":"added","patch":"@@ -0,0 +2 @@\n+assert()"}
    ])).unwrap();
    futures_lite::future::block_on(async {
        let text = prompt(PR, &head, "Explain the fallback behavior", true);
        let plan = run(exe.as_os_str(), Some(&root), &text, &files, |_| {})
            .await
            .unwrap();
        let route = locate(plan, PR, &head, &files).unwrap();
        assert_eq!(
            route.iter().map(|s| s.location).collect::<Vec<_>>(),
            [
                Ok((0, 2)),
                Ok((0, 1)),
                Ok((1, 1)),
                Err("Line is not on this side of the open diff")
            ]
        );
        let plan = run(exe.as_os_str(), Some(&root), &text, &files, |_| {})
            .await
            .unwrap();
        assert!(locate(plan, PR, "changed", &files).is_err());
        let fail = executable(tools.path(), "exit 17");
        assert!(
            run(fail.as_os_str(), Some(&root), &text, &files, |_| {})
                .await
                .is_err()
        );
        assert!(
            run(
                tools.path().join("missing").as_os_str(),
                Some(&root),
                &text,
                &files,
                |_| {}
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn reads_bundle_without_inlining_patches_or_requiring_checkout() {
    let files: Vec<File> = serde_json::from_value(serde_json::json!([
        {"filename":"service.ts","sha":"1","status":"added","patch":"@@ -0,0 +1 @@\n+export const fallback = true;"}
    ])).unwrap();
    let tools = tempfile::tempdir().unwrap();
    let exe = executable(
        tools.path(),
        r#"
test "$PWD" = "$FERRISCOPE_PR_BUNDLE"
echo '{"type":"item.started","item":{"type":"command_execution"}}'
echo '{"type":"item.completed","item":{"type":"command_execution","aggregated_output":"private patch"}}'
grep -F service.ts manifest.json >/dev/null
grep -F "+export const fallback" 0.patch >/dev/null
case " $* " in *" --skip-git-repo-check "*) ;; *) exit 1;; esac
while [ "$1" != --output-last-message ]; do shift; done
input=$(cat)
printf '%s' "$input" | grep -F 'manifest.json' >/dev/null
if printf '%s' "$input" | grep -F '+export const fallback'; then exit 13; fi
echo '{"pr":"demo/service/pull/1","head":"rev","stops":[{"path":"service.ts","side":"RIGHT","line":1,"why":"Defines the fallback."}]}' > "$2"
"#,
    );
    let text = prompt(PR, "rev", FLOW, false);
    let mut counts = Vec::new();
    let plan = futures_lite::future::block_on(run(exe.as_os_str(), None, &text, &files, |n| {
        counts.push(n)
    }))
    .unwrap();
    assert_eq!(counts, [1]);
    assert_eq!(
        locate(plan, PR, "rev", &files).unwrap()[0].location,
        Ok((0, 1))
    );
}

#[test]
fn cancelling_generation_drops_the_cli_process() {
    use std::time::Duration;
    let tmp = tempfile::tempdir().unwrap();
    let exe = executable(
        tmp.path(),
        r#"echo $$ > "$FERRISCOPE_PR_SOURCE/pid"; exec sleep 60"#,
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let future = run(exe.as_os_str(), Some(tmp.path()), "fixture", &[], |_| {});
        assert!(
            tokio::time::timeout(Duration::from_millis(400), future)
                .await
                .is_err()
        );
        let pid = fs::read_to_string(tmp.path().join("pid")).unwrap();
        for _ in 0..30 {
            if !std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("CLI was still running after cancellation");
    });
}
