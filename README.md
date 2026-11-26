# <img src="assets/ferriscope.svg" width="40" height="40" alt=""> Ferriscope

Review a GitHub pull request by behavior, not alphabetical path. Put a handler beside its test, skip the lockfile, and resume where you left off.

**For private repositories or publishing reviews, use the desktop app.** The browser is a public PR reader. It cannot sign in, even if you’re signed in to github.com in another tab.

| What you want to do | Desktop | Browser |
| --- | --- | --- |
| Read public PRs, search files, organize diffs, track progress | Yes | Yes |
| Read private PRs | With your `gh` login | No |
| Post inline comments, approve, or request changes | Yes | No |

## Install the desktop app

### Linux x86-64: download the release

Install [Ferriscope v0.1.0](https://github.com/tvanboxtel/ferriscope/releases/tag/v0.1.0) without Rust or `sudo`:

```sh
curl -fLO https://github.com/tvanboxtel/ferriscope/releases/download/v0.1.0/ferriscope-0.1.0-linux-x86_64.tar.gz
tar -xzf ferriscope-0.1.0-linux-x86_64.tar.gz
sh ferriscope-0.1.0-linux-x86_64/install.sh
```

Open **Ferriscope** from your applications menu or run `~/.local/bin/ferriscope`. The binary was built on Arch Linux and requires compatible GTK 3 and WebKitGTK 4.1 runtime libraries. Sign in with `gh auth login` to open PRs. See the [latest release](https://github.com/tvanboxtel/ferriscope/releases/latest) for updates or the [uninstall instructions](packaging/linux/INSTALL.txt) to remove it.

### Build from source

**Testing the Apple Silicon Mac build:** the [macOS desktop workflow](https://github.com/tvanboxtel/ferriscope/actions/workflows/macos.yml) builds a `.app` and checks that it launches before uploading an artifact. From a successful run, download **Ferriscope-macOS-Apple-Silicon** while signed in to GitHub. Extract the Actions download and the app ZIP inside it, then drag **Ferriscope.app** into Applications. This is an ad-hoc signed test build, not Developer ID signed or notarized; Gatekeeper may block it. No Mac binary is included in the original Linux release.

**Install or update to the latest `main`**, from any directory:

```sh
cargo +1.100.0 install --git https://github.com/tvanboxtel/ferriscope.git --branch main --locked --features desktop ferriscope
```

Then run **`ferriscope`**. Run the same install command later to update. No clone, `cargo run`, or Dioxus CLI needed.

This builds an optimized binary locally. You’ll need [rustup](https://rustup.rs/) (`rustup toolchain install 1.100.0`), the [desktop build dependencies](https://dioxuslabs.com/learn/0.7/getting_started/), and Cargo’s bin directory on your `PATH`. Sign in to [GitHub CLI](https://cli.github.com/) with `gh auth login` before opening a PR.

**macOS:** The native Dioxus desktop app targets macOS too. Install Xcode Command Line Tools (`xcode-select --install`), Rust, and `gh`, then use the same Cargo command above. This installs a terminal-launchable executable, not a signed `.app` or `.dmg`. The prebuilt Linux archive does not run on macOS. Linux is tested here; the macOS build has not been verified.

<details>
<summary>Package your own Linux binary</summary>

**To build a shareable Linux archive** from this repo, run `sh packaging/linux/package.sh`. Send the resulting `.tar.gz` in `target/dist/`, not the source tree. Recipients use their own `gh` login.

</details>

## Run from a checkout (development)

From the repo root:

```sh
cargo run --locked --features desktop
```

When you see `Running target/debug/ferriscope`, **switch to the Ferriscope window**. Paste a PR URL into the field at the top and click **Open / refresh**. The URL goes in the app, not the terminal.

On Linux, `cargo run` and `cargo install` do not register a desktop launcher. If your Wayland taskbar shows a generic icon, use the Linux archive installer above, close Ferriscope, then launch it from the applications menu.

<details>
<summary><strong>First time? Install the prerequisites and get the source</strong></summary>

1. Install [Rust via rustup](https://rustup.rs/), [GitHub CLI](https://cli.github.com/), and the [Dioxus desktop dependencies for your OS](https://dioxuslabs.com/learn/0.7/getting_started/). Linux needs GTK 3 and WebKitGTK 4.1 development libraries. Desktop does **not** need the Dioxus CLI.
2. Clone the repo:
   ```sh
   git clone https://github.com/tvanboxtel/ferriscope.git
   cd ferriscope
   ```
3. Run `gh auth status --hostname github.com`. If needed, sign in with `gh auth login --hostname github.com`.
4. Run the launch command above. The first build downloads the pinned toolchain and compiles dependencies.

</details>

## Run the public PR reader in a browser

**Once**, from the repo root with rustup installed:

```sh
cargo install dioxus-cli --version '=0.7.10' --locked --root target/tools
```

**Each launch:**

```sh
target/tools/bin/dx serve --platform web --port 8080
```

Open [localhost:8080](http://127.0.0.1:8080), then paste a **public** PR URL inside Ferriscope. Keep the terminal open; `Ctrl+C` stops the server. No `PATH` change is required. Anonymous GitHub API rate limits apply.

## At the keyboard

On desktop, **Check for updates** next to **Appearance** compares your installed version with the latest GitHub release. If a newer release exists, choose **View release** for its downloads. The check runs only when clicked; it never installs software or checks for unreleased commits.

| Key | Action |
| --- | --- |
| `/` | Search all changed file paths, including collapsed groups |
| `Enter` / `Esc` in search | Open the first match / return to the tree |
| `J` / `K` | Next / previous file in review order |
| `Space` | Toggle the active file’s local reviewed status |

**Organize:** check related files, name the group, and choose **Review together**. Use the arrows to reorder. Reopen the same PR to resume; after a push, only reviewed files whose diff changed need another look. Progress is local. In the browser, use the same profile, host, and port.

**Publish (desktop):** use the numbered `+` beside a diff line to save an inline draft, or open **Review changes** directly. Choose **Comment**, **Approve**, or **Request changes**, then submit. Marking a file reviewed does **not** submit a GitHub review. GitHub won’t let you approve or request changes on your own PR.

<details>
<summary><strong>Development and performance checks</strong></summary>

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

The workflow test uses `tests/mixed.json`; set `LIVE=1` to also open a real public PR. The performance fixture uses 600 files, a 3,000-line Rust patch, and 100 ms simulated API latency per request.

</details>
