# Ferriscope

A local-first GitHub PR reader. Paste a `github.com` pull request URL, read a diff, then select related files with their checkboxes. Give the selection a name and click **Review together**. The arrows reorder groups or files. **Mark reviewed** saves immediately; reopen the same URL or choose **Open / refresh** after a push.

## Run

Use rustup: `rust-toolchain.toml` selects Rust **1.100.0** and the WASM target. The lockfile and manifest pin Dioxus **0.7.10**. Install the matching CLI into this checkout:

```sh
cargo install dioxus-cli --version '=0.7.10' --locked --root target/tools
target/tools/bin/dx serve --platform web
```

Open the localhost URL printed by `dx`. Browser access is anonymous, for public PRs only, and subject to GitHub's unauthenticated API quota.

For desktop, install the [Dioxus desktop system dependencies](https://dioxuslabs.com/learn/0.7/getting_started/) (on Linux, including GTK 3 and WebKitGTK 4.1) and the GitHub CLI. Use a `gh` login with read access to the repository:

```sh
gh auth status --hostname github.com
cargo run --locked --features desktop
```

Try `https://github.com/octocat/Hello-World/pull/1` for a small live diff. Both platforms fetch fresh metadata and all pages of files on open, and reject a changing snapshot or a PR beyond GitHub's 3,000-file limit rather than silently dropping files. Binary and omitted patches link out to GitHub.

## How review state works

In the reader, press **J** for the next file or **K** for the previous file, or use the buttons above the diff. Navigation follows your group and file order and opens the destination group. Shortcuts do not intercept text typed into the URL or group name fields. Diffs highlight recognized source extensions locally; other files retain plain unified-diff coloring.

Classification uses the first case-insensitive path match: generated directories and lockfiles, test directories or filename conventions, documentation, infrastructure (including migrations), known source extensions, then Other. The complete rules are in `src/lib.rs`. Every group starts collapsed, including saved custom groups. Expand a group to see its files, then choose a file to open its diff.

Each PR has its own ordered groups, paths and reviewed fingerprints. The browser stores only this small JSON record in localStorage for the current origin. Desktop atomically replaces a JSON file in the OS local data directory under `ferriscope/`. Neither stores tokens or patch contents. Use the same browser profile and origin to resume.

A reviewed fingerprint covers the patch, change type and rename source, falling back to the blob SHA when no patch is supplied. It does not include the PR head SHA. New paths enter their default groups; disappeared paths leave the list. A path renamed between refreshes is treated as new.

## Tests

Default file groups show collapsible directory sections; custom groups keep their manual sequence. With the reader focused, **Space** toggles the active file’s local reviewed status. Space on a focused button or checkbox still activates that control.

**Desktop inline reviews:** select the numbered `+` beside a patch line, write a note, and choose **Save draft**. Saved comments survive reopening the PR. **Submit … inline comments to GitHub** publishes the batch as one comment-only review via your `gh` login (not an approval). If the PR head changes, copy the old notes and re-anchor them before submitting. The browser remains read-only on GitHub.

```sh
cargo test --locked --features desktop
```

`tests/mixed.json` contains a handler, its test, a migration, docs, a lockfile and a binary. The Rust acceptance test saves and reloads the arrangement, changes only the test patch, and checks that the handler stays reviewed.

For a browser-level check, start the web build above, then run (with Node and a Chromium executable installed):

```sh
npm install --prefix target/browser-test playwright-core
NODE_PATH="$PWD/target/browser-test/node_modules" CHROME=/path/to/chromium \
  APP_URL=http://127.0.0.1:8080 node tests/browser.cjs
```

The browser test intercepts only the fixture PR's GitHub API calls. Set `LIVE=1` to also read a public GitHub diff before running the fixture workflow.
