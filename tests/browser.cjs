// Optional UI smoke test; see README for setup.
const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");
const files = require("./mixed.json");

async function run() {
  const browser = await chromium.launch({ executablePath: process.env.CHROME });
  try {
    const page = await browser.newPage();
    await page.goto(process.env.APP_URL || "http://127.0.0.1:8080");
    async function open(url, heading) {
      await page.getByLabel("GitHub PR URL").fill(url);
      await page.getByText("Open / refresh").click();
      await page.getByRole("heading", { name: heading, exact: true }).waitFor();
      assert.equal(await page.locator("[aria-expanded=true]").count(), 0);
    }
    if (process.env.LIVE) {
      await open("https://github.com/octocat/Hello-World/pull/1", "octocat/hello-world/pull/1");
      await page.getByRole("button", {name: /^Docs/}).click();
      await page.getByRole("button", {name: /^README/}).click();
      assert.match(await page.locator("article pre").innerText(), /@@/);
    }
    const url = "https://github.com/demo/repo/pull/2";
    let longDiff = false;
    await page.route("https://api.github.com/repos/demo/repo/**", route =>
      route.fulfill({ json: route.request().url().includes("/files?")
        ? (longDiff ? files.map((file, i) => i === 1 ? {
          ...file, sha: "long", additions: 140, patch: "@@ -0,0 +1,140 @@\n" +
            Array.from({length:140}, (_, n) => `+let row_${n} = ${n};`).join("\n"),
        } : file) : files)
        : { base: { sha: "base" }, head: { sha: longDiff ? "long" : files[2].sha }, changed_files: files.length } }));
    await open(url, "demo/repo/pull/2");
    assert.equal(await page.getByLabel("Select Cargo.lock").count(), 0);
    await page.keyboard.press("/");
    const search=page.getByRole("searchbox",{name:"Search files"});
    await search.fill("MISSING/j k");
    await page.getByText(/No files match/).waitFor();
    await search.fill("CARGO.LOCK");
    const lockResult = page.getByRole("button", {name: /^Cargo.lock/});
    const lockCounts = lockResult.getByRole("img", {name:"Lines added: 1; lines removed: 1"});
    assert.equal(await lockCounts.locator(".lines-added").innerText(), "+1");
    assert.equal(await lockCounts.locator(".lines-removed").innerText(), "−1");
    await search.press("Escape");
    assert.equal(await page.getByLabel("Select Cargo.lock").count(), 0);
    await page.keyboard.press("j");
    await page.getByRole("heading", {name:"src/accounts.rs"}).waitFor();
    const counts = {name:"Lines added: 1; lines removed: 0"};
    await page.locator("article").getByRole("img", counts).waitFor();
    await page.getByRole("button", {name:/^src\/accounts.rs/}).getByRole("img", counts).waitFor();
    assert.match(await page.locator("pre .add").innerText(), /^\+fn delete_account\(\) \{\}$/);
    assert.ok(await page.locator("pre .add span[style]").count() > 1);
    const token = page.locator("pre .add span[style]").first();
    const lightColor = await token.evaluate(el => getComputedStyle(el).color);
    const settings = page.getByLabel("Ferriscope settings");
    await settings.click();
    const panel = await page.locator(".settings-panel").boundingBox();
    const header = await page.locator("header").boundingBox();
    const title = await page.getByText("Color theme", {exact:true}).boundingBox();
    assert.ok(panel.y >= header.y + header.height && title.y >= panel.y &&
      panel.y + panel.height <= page.viewportSize().height, "Themes should open below the app menu, in view");
    await page.getByRole("radio", {name:"Dark",exact:true}).check();
    await page.waitForFunction(color => getComputedStyle(document.querySelector("pre .add span")).color !== color, lightColor);
    await page.getByRole("radio", {name:"Dusk"}).check();
    await settings.click();
    await page.locator("main").focus();
    await page.keyboard.press("j");
    await page.getByRole("heading", {name:"tests/accounts.rs"}).waitFor();
    await page.keyboard.press("k");
    await page.getByRole("heading", {name:"src/accounts.rs"}).waitFor();
    await page.keyboard.press("Space");
    await page.getByRole("button",{name:/Mark unread/}).waitFor();
    await page.keyboard.press("Space");
    await page.getByRole("button",{name:/Mark reviewed/}).waitFor();
    const folder=page.locator("button.folder").filter({hasText:"src /"});
    await folder.click();
    assert.equal(await page.getByLabel("Select src/accounts.rs").count(),0);
    await folder.click();
    await page.getByRole("button", { name: /^Generated\/lockfiles/ }).click();
    await page.getByLabel("Select Cargo.lock").waitFor();
    await page.getByRole("button", {name: /^src\/accounts.rs/}).click();
    await page.getByLabel("Select src/accounts.rs").check();
    await page.getByLabel("Select tests/accounts.rs").check();
    await page.getByLabel("Group name").fill("Account deletion");
    await page.getByText("Review together (2)").click();
    await page.getByRole("button", {name: /^Account deletion/}).click();
    await page.getByText("Mark reviewed").click();
    await page.getByRole("button", { name: /tests\/accounts.rs/ }).click();
    await page.getByText("Mark reviewed").click();
    await page.reload();
    await settings.click();
    assert.ok(await page.getByRole("radio", {name:"Dusk"}).isChecked());
    await page.getByRole("button", {name:"Recent PRs"}).click();
    await page.getByRole("button", {name:"demo/repo/pull/2",exact:true}).click();
    await page.getByRole("heading", {name:"demo/repo/pull/2"}).waitFor();
    await page.getByRole("button", {name: /^Account deletion/}).click();
    await page.getByText("Tests · Reviewed", { exact: true }).waitFor();
    files[2].sha = "new";
    files[2].patch += "\n+// changed";
    await page.getByText("Open / refresh").click();
    const toggle = page.getByRole("button", {name: /^Account deletion/});
    await page.locator("button[aria-expanded=false]").filter({hasText:"Account deletion"}).waitFor();
    await toggle.click();
    const group = page.locator("section").filter({ has: toggle });
    await group.getByText("Tests · Changed", { exact: true }).waitFor();
    await group.getByText("Implementation · Reviewed", { exact: true }).waitFor();
    await page.getByRole("button", {name:"Recent PRs"}).click();
    assert.equal(await page.getByRole("button",{name:"demo/repo/pull/2",exact:true}).count(),1);
    await page.getByRole("button", {name:"Clear history"}).click();
    await page.getByText("No recently opened PRs yet.").waitFor();
    await group.getByText("Implementation · Reviewed", { exact: true }).waitFor();
    await page.getByRole("button", {name:"Recent PRs"}).click();
    await page.getByRole("button", {name:"Remove group Account deletion"}).click();
    await toggle.waitFor({state:"detached"});
    await page.reload();
    await open(url, "demo/repo/pull/2");
    assert.equal(await toggle.count(),0);
    await page.getByRole("button",{name:/^Implementation/}).click();
    await page.getByText("Implementation · Reviewed",{exact:true}).waitFor();
    await page.getByRole("button",{name:/^Tests/}).click();
    await page.getByText("Tests · Changed",{exact:true}).waitFor();
    longDiff = true;
    await page.getByText("Open / refresh").click();
    await page.locator("article .empty").waitFor();
    await page.locator("main").press("j");
    const diff = page.locator(".diff-scroll");
    const fileHeading = page.locator(".file-heading");
    await fileHeading.getByRole("img", {name:"Lines added: 140; lines removed: 0"}).waitFor();
    for (const width of [1100, 640]) {
      await page.setViewportSize({width, height:720});
      await diff.evaluate(el => { el.scrollTop = 0; });
      const before = await fileHeading.boundingBox();
      await diff.evaluate(el => { el.scrollTop = el.scrollHeight; });
      assert.ok(await diff.evaluate(el => el.scrollTop) > 0, "Long diff should scroll inside its pane");
      assert.deepEqual(await fileHeading.boundingBox(), before, "File heading and actions should not move with the diff");
    }
    console.log("Browser review workflow passed");
  } finally {
    await browser.close();
  }
}
run().catch(error => { console.error(error); process.exitCode = 1; });
