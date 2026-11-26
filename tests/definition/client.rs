use super::*;
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    head: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("checkout with space");
        fs::create_dir(&root).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(&root)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{:?}", out.stderr);
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        run(&["init", "-q"]);
        fs::write(root.join("library.ts"), include_str!("library.ts")).unwrap();
        fs::write(root.join("tsconfig.json"), include_str!("tsconfig.json")).unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "base"]);
        fs::write(root.join("caller.ts"), include_str!("caller.ts")).unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "caller"]);
        let head = run(&["rev-parse", "HEAD"]);
        let fake = dir.path().join("server");
        fs::write(&fake, include_str!("fake-lsp.py")).unwrap();
        fs::set_permissions(fake, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, root, head }
    }
    fn call(&self, head: &str, server: &Path) -> ResultOf<Source> {
        let s = Selection {
            line: 3,
            before: "export const value = [\"🦀\", ".into(),
            symbol: "alias".into(),
            after: "()];".into(),
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                verify(&self.root, head).await?;
                lookup(
                    &Default::default(),
                    &self.root,
                    server.to_str().unwrap(),
                    head,
                    "caller.ts",
                    &s,
                )
                .await
            })
    }
}
#[test]
fn alias_positions_and_scope() {
    let f = Fixture::new();
    let fake = f.dir.path().join("server");
    for mode in ["utf-16", "utf-8", "utf-32", "none", "outside"] {
        fs::write(f.dir.path().join("mode"), mode).unwrap();
        let result = f.call(&f.head, &fake);
        match mode {
            "none" => assert!(result.err().unwrap().contains("No definition")),
            "outside" => assert!(result.err().unwrap().contains("outside")),
            _ => {
                let src = result.unwrap();
                assert_eq!((src.path.as_str(), src.line), ("library.ts", 1));
                assert_eq!(src.text, include_str!("library.ts"));
            }
        }
    }
    let missing = f.dir.path().join("missing");
    assert!(
        f.call("wrong", &missing)
            .err()
            .unwrap()
            .contains("PR commit")
    );
    assert!(
        f.call(&f.head, &missing)
            .err()
            .unwrap()
            .contains("Cannot start")
    );
    fs::write(f.root.join("library.ts"), "changed").unwrap();
    assert!(
        f.call(&f.head, &missing)
            .err()
            .unwrap()
            .contains("local changes")
    );
}
#[test]
#[ignore = "requires FERRISCOPE_TEST_LSP pointing to a TypeScript stdio server"]
fn real_typescript_definition() {
    let f = Fixture::new();
    let server = std::env::var_os("FERRISCOPE_TEST_LSP").unwrap();
    let src = f.call(&f.head, Path::new(&server)).unwrap();
    assert_eq!((src.path.as_str(), src.line), ("library.ts", 1));
}
