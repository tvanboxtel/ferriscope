# <img src="assets/ferriscope.svg" width="40" height="40" alt=""> Ferriscope

**Review GitHub pull requests in your own order.** Read a handler beside its test, skip the lockfile, and pick up where you left off. When you’re ready, send your review to GitHub.

## TL;DR — install and open a PR

**No Rust or source checkout needed.** Pick your [v0.2.1 download](https://github.com/tvanboxtel/ferriscope/releases/tag/v0.2.1):

| Your computer | Download | Install |
| --- | --- | --- |
| **Mac with Apple Silicon** (M-series) | [Ferriscope for Mac (.zip)](https://github.com/tvanboxtel/ferriscope/releases/download/v0.2.1/Ferriscope-macOS-arm64.zip) | Unzip, drag **Ferriscope.app** into **Applications**, then open it. |
| **Linux x86-64** | [Ferriscope for Linux (.tar.gz)](https://github.com/tvanboxtel/ferriscope/releases/download/v0.2.1/ferriscope-0.2.1-linux-x86_64.tar.gz) | Check the runtime requirements below, extract, and run `sh install.sh` inside the extracted folder. Then open **Ferriscope** from the applications menu. |

**First launch:** Ferriscope uses your [GitHub CLI](https://cli.github.com/) login. If you haven’t signed in, run `gh auth login --hostname github.com` in a terminal. Pick a PR from **Your PRs** or paste a PR URL and click **Open / refresh**. Press **J** to start reading.

<details>
<summary>Mac won’t open the app?</summary>

This build is ad-hoc signed, **not Apple notarized**. Gatekeeper may block it. If you trust the download, follow [Apple’s instructions for opening an app from an unidentified developer](https://support.apple.com/en-us/102445); don’t disable Gatekeeper. On a managed work Mac, follow your organization’s policy. There’s no prebuilt Intel Mac or Windows download in this release.

</details>

<details>
<summary>Linux: copy-paste install, requirements, and updates</summary>

The archive needs compatible GTK 3 and WebKitGTK 4.1 runtime libraries. It includes libxdo, so you don't need to install `xdotool`. If an older download fails with a `libxdo.so.3` error, install the current archive below.

Run from any directory:

```sh
curl -fLO https://github.com/tvanboxtel/ferriscope/releases/download/v0.2.1/ferriscope-0.2.1-linux-x86_64.tar.gz &&
tar -xzf ferriscope-0.2.1-linux-x86_64.tar.gz &&
sh ferriscope-0.2.1-linux-x86_64/install.sh
```

The installer registers the launcher and icon and places the executable at `~/.local/bin/ferriscope` without `sudo`. To update, close the app and install a compatible [release](https://github.com/tvanboxtel/ferriscope/releases/latest) the same way; saved reviews stay on disk. [Uninstall instructions](packaging/linux/INSTALL.txt).

</details>

## Your first review

**Guided review (desktop):** Open a PR, choose **Follow the data flow** or **Custom walkthrough**, and inspect **Preview prompt**. Only **Generate walkthrough** starts the agent. It receives a temporary bundle containing a changed-file manifest and the available GitHub patches. For unchanged callers or tests, select **Include full PR source** to download a temporary snapshot pinned to the PR commit. Large repositories take longer. No local checkout is needed.

Choose the agent once in **Ferriscope settings → Guided review agent**:

- **Local Codex CLI** (default) uses `codex` and its existing login. Ferriscope requests a read-only shell; configured Codex integrations may have broader permissions.
- **Local ACP agent** uses an already-installed, locally authenticated [ACP v2 Preview](https://agentclientprotocol.com/protocol/v2/migration) stdio agent with session support. Select a detected adapter (for example, **Claude Agent — installed by Zed**) and click **Save agent**. Ferriscope remembers the installation source, so a new version in Zed’s registry package directory does not need a new path. Discovery checks local files, not protocol compatibility; only **Generate walkthrough** starts the adapter. If yours isn’t listed, browse for the executable or enter its name or absolute path. Under **Advanced**, enter any arguments one per line, without shell quotes. Ferriscope does not install adapters, sandbox the agent, grant permission requests, or fall back to Codex. A v1-only adapter cannot generate a walkthrough.

Generation can send PR context to the chosen agent’s provider and consume its quota. **Raw review** needs neither agent. When generation succeeds, open **Walkthrough** under **File Groups** and use **Previous / Next** or **K / J** to follow the stops. Select another file to resume normal navigation. Review marks and drafts are independent of the in-memory route.

- **See what’s waiting:** The desktop **Your PRs** inbox shows open pull requests that directly request your review, followed by other open PRs assigned to you. A PR can have both labels. Click **Home** in the reader to see the inbox, then **Back to reader** to pick up where you left off.
- **Find a file:** press `/` to search every changed path, including collapsed groups. **Enter** opens the first match; **Esc** returns to the tree.
- **Read related changes together:** select files with their checkboxes, name the group, and click **Review together**. Use the arrows to reorder. **Ungroup** removes a custom group and returns its files to their default groups without losing review progress.
- **Keep your place:** **J / K** move forward and back; **Space** marks the current file reviewed or unread. **Recent PRs** reopens work on this device. After a push, only reviewed files whose diff changed need another look.
- **Send feedback:** click the numbered `+` beside a line to draft a comment in the pane below the diff. From that pane, choose **Approve**, **Request changes**, or **Post comments** when you’re ready. All saved inline comments are included; review the summary and confirm to publish on GitHub. Marking a file reviewed does *not* submit a review. GitHub doesn’t allow approving or requesting changes on your own PR.

The **Ferriscope ▾** menu holds appearance settings and the desktop release checker. **Check for updates** links to newer releases; it doesn’t replace the running app.

## Other ways to run Ferriscope

### Go to definition (desktop)

In the top toolbar, open **Code navigation** and choose the local repository folder. Confirm the GitHub repository and detected TypeScript server, then click **Trust and connect**. You can do this on any branch, before opening a PR. Skip it entirely if you just want to review.

In a connected repo, **Ctrl-click** a TypeScript identifier (**⌘-click** on macOS) on an added or context line. Ferriscope runs the trusted server in a temporary checkout at the displayed PR commit, not on your working files. You can stay on your branch with uncommitted changes. To fetch missing PR commits automatically, enable **Fetch missing PR commits automatically** in **Code navigation**. This is off by default, including for previously connected repos. Opening a PR with TypeScript changes prepares its temporary checkout in the background. With fetch permission, this also downloads a missing commit before you select a file; without permission, preparation stays local. An early Ctrl-click waits for the same preparation rather than starting it again. By default the first lookup starts the language server. To overlap startup and TypeScript project loading with reading, enable **Start TypeScript indexing when opening a PR** for the connected repository. This runs the trusted server in the background after checkout preparation. All jumps in that reader reuse the server, including Ctrl-clicks inside definition source. **Previous definition** retraces a jump; **Back to diff** immediately returns to the review. Ferriscope does not install dependencies there. **Back to diff** preserves your place and draft. The connection and fetch permission are saved per repository across restarts. To disconnect, open **Code navigation** and choose **Disconnect checkout**.

To exercise the round trip on Linux, run `cargo build --locked --features desktop`, then `xvfb-run -a -s '-screen 0 1280x960x24' node tests/definition/desktop.mjs` (Node 22+, Python 3, Xvfb, xdotool). It creates a temporary checkout and mocks GitHub and the LSP server. Set `FERRISCOPE_TEST_LSP` to an existing TypeScript server executable's absolute path to run the same desktop scenario against that server.

<details>
<summary>Build the latest desktop version from source</summary>

Install [rustup](https://rustup.rs/), [GitHub CLI](https://cli.github.com/), and the [Dioxus desktop build dependencies for your OS](https://dioxuslabs.com/learn/0.7/getting_started/). On macOS, install Xcode Command Line Tools (`xcode-select --install`); on Linux, install the GTK 3, WebKitGTK 4.1, and libxdo development files (on Arch, `gtk3`, `webkit2gtk-4.1`, and `xdotool`).

```sh
rustup toolchain install 1.100.0
cargo +1.100.0 install --git https://github.com/tvanboxtel/ferriscope.git --branch main --locked --features desktop ferriscope
```

Run `ferriscope` with Cargo’s bin directory on your `PATH`. This installs a terminal executable, not a graphical launcher. For development in a checkout, use `cargo run --locked --features desktop`.

To build the shareable Linux archive, run `sh packaging/linux/package.sh`. Use Debian 12 for the release build to avoid requiring a newer glibc. Install `curl`, `make`, `pkg-config`, `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libssl-dev`, `libx11-dev`, `libxtst-dev`, `libxinerama-dev`, and `libxkbcommon-dev`. The packaging script builds libxdo itself rather than using the system copy.

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
