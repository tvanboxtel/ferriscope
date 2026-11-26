# <img src="assets/ferriscope.svg" width="40" height="40" alt=""> Ferriscope

**Review GitHub pull requests in your own order.** Read a handler beside its test, skip the lockfile, and pick up where you left off. When you’re ready, send your review to GitHub.

## TL;DR — install and open a PR

**No Rust or source checkout needed.** Pick your [v0.1.2 download](https://github.com/tvanboxtel/ferriscope/releases/tag/v0.1.2):

| Your computer | Download | Install |
| --- | --- | --- |
| **Mac with Apple Silicon** (M-series) | [Ferriscope for Mac (.zip)](https://github.com/tvanboxtel/ferriscope/releases/download/v0.1.2/Ferriscope-macOS-arm64.zip) | Unzip, drag **Ferriscope.app** into **Applications**, then open it. |
| **Linux x86-64** | [Ferriscope for Linux (.tar.gz)](https://github.com/tvanboxtel/ferriscope/releases/download/v0.1.2/ferriscope-0.1.2-linux-x86_64.tar.gz) | Extract, run `sh install.sh` inside the extracted folder, then open **Ferriscope** from the applications menu. No `sudo`. |

**First launch:** Ferriscope uses your [GitHub CLI](https://cli.github.com/) login. If you haven’t signed in, run `gh auth login --hostname github.com` in a terminal. Then paste a PR URL **in the Ferriscope window** and click **Open / refresh**. Press **J** to start reading.

<details>
<summary>Mac won’t open the app?</summary>

This build is ad-hoc signed, **not Apple notarized**. Gatekeeper may block it. If you trust the download, follow [Apple’s instructions for opening an app from an unidentified developer](https://support.apple.com/en-us/102445); don’t disable Gatekeeper. On a managed work Mac, follow your organization’s policy. There’s no prebuilt Intel Mac or Windows download in this release.

</details>

<details>
<summary>Linux: copy-paste install, requirements, and updates</summary>

Run from any directory:

```sh
curl -fLO https://github.com/tvanboxtel/ferriscope/releases/download/v0.1.2/ferriscope-0.1.2-linux-x86_64.tar.gz &&
tar -xzf ferriscope-0.1.2-linux-x86_64.tar.gz &&
sh ferriscope-0.1.2-linux-x86_64/install.sh
```

The binary was built on Arch Linux and needs compatible GTK 3 and WebKitGTK 4.1 runtime libraries. The installer registers the launcher and icon and places the executable at `~/.local/bin/ferriscope`. To update, close the app and install the [latest release](https://github.com/tvanboxtel/ferriscope/releases/latest) the same way; saved reviews stay on disk. [Uninstall instructions](packaging/linux/INSTALL.txt).

</details>

## Your first review

- **Find a file:** press `/` to search every changed path, including collapsed groups. **Enter** opens the first match; **Esc** returns to the tree.
- **Read related changes together:** select files with their checkboxes, name the group, and click **Review together**. Use the arrows to reorder. **Ungroup** removes a custom group and returns its files to their default groups without losing review progress.
- **Keep your place:** **J / K** move forward and back; **Space** marks the current file reviewed or unread. **Recent PRs** reopens work on this device. After a push, only reviewed files whose diff changed need another look.
- **Send feedback:** draft inline comments with the numbered `+` beside a line. Above the diff, choose **Approve**, **Request changes**, or **Post comments**. All saved inline comments are included; review the summary and confirm to publish on GitHub. Marking a file reviewed does *not* submit a review. GitHub doesn’t allow approving or requesting changes on your own PR.

The **Ferriscope ▾** menu holds appearance settings and the desktop release checker. **Check for updates** links to newer releases; it doesn’t replace the running app.

## Other ways to run Ferriscope

<details>
<summary>Build the latest desktop version from source</summary>

Install [rustup](https://rustup.rs/), [GitHub CLI](https://cli.github.com/), and the [Dioxus desktop build dependencies for your OS](https://dioxuslabs.com/learn/0.7/getting_started/). On macOS, install Xcode Command Line Tools (`xcode-select --install`); on Linux, install the GTK 3 and WebKitGTK 4.1 development libraries.

```sh
rustup toolchain install 1.100.0
cargo +1.100.0 install --git https://github.com/tvanboxtel/ferriscope.git --branch main --locked --features desktop ferriscope
```

Run `ferriscope` with Cargo’s bin directory on your `PATH`. This installs a terminal executable, not a graphical launcher. For development in a checkout, use `cargo run --locked --features desktop`. To build a shareable Linux archive, run `sh packaging/linux/package.sh`.

</details>

<details>
<summary>Run the browser version (public PRs only)</summary>

The browser can read, search, group, and track **public** PRs. It can’t use your `gh` login, open private PRs, or submit reviews. For those, use desktop.

From the repo root with rustup installed, install the CLI **once**:

```sh
cargo install dioxus-cli --version '=0.7.10' --locked --root target/tools
```

Each launch:

```sh
target/tools/bin/dx serve --platform web --port 8080
```

Open [127.0.0.1:8080](http://127.0.0.1:8080). Use the same browser profile, host, and port to resume your reviews. Anonymous GitHub API limits apply.

</details>

<details>
<summary>Development and performance checks</summary>

```sh
cargo test --locked --features desktop
```

With the web server running, Node installed, and `CHROME` pointing to a Chromium executable:

```sh
npm install --prefix target/browser-test playwright-core
export NODE_PATH="$PWD/target/browser-test/node_modules"
export CHROME=/path/to/chromium
export APP_URL=http://127.0.0.1:8080
node tests/browser.cjs
CHECK_BOUNDS=1 node tests/performance.cjs
```

The workflow test uses `tests/mixed.json`; set `LIVE=1` to also open a real public PR. The performance check uses 600 files, a 3,000-line Rust patch, and 100 ms simulated latency per request.

To profile an everyday PR, run `CHECK_CACHE=1 node tests/everyday.cjs` against the release web build. It reports five fresh-tab trials for opening six files, showing a highlighted 24-line diff, and refreshing. Only the current PR’s verified patches are reused, in memory, after checking its base SHA, head SHA, and file count with GitHub.

For a release-mode stress test, serve the web app with `--release` and run `FILES=3000 CHECK_BOUNDS=1 node tests/performance.cjs`. It times opening, diff navigation, path search, jumping to the final file, and marking it reviewed. It also checks the number of rendered rows; timing alone varies by machine. To measure the native comment preview separately, run `cargo test --release --locked --features desktop measure_fenced_preview -- --ignored --nocapture`.

</details>
