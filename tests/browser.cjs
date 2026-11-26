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
    }
    if (process.env.LIVE) {
      await open("https://github.com/octocat/Hello-World/pull/1", "README");
      assert.match(await page.locator("article pre").innerText(), /@@/);
    }
    const url = "https://github.com/demo/repo/pull/2";
    await page.route("https://api.github.com/repos/demo/repo/**", route =>
      route.fulfill({ json: route.request().url().includes("/files?")
        ? files : { base: { sha: "base" }, head: { sha: files[2].sha }, changed_files: files.length } }));
    await open(url, "src/accounts.rs");
    assert.equal(await page.getByLabel("Select Cargo.lock").count(), 0);
    await page.getByRole("button", { name: /Generated\/lockfiles · 1/ }).click();
    await page.getByLabel("Select Cargo.lock").waitFor();
    await page.getByLabel("Select src/accounts.rs").check();
    await page.getByLabel("Select tests/accounts.rs").check();
    await page.getByLabel("Group name").fill("Account deletion");
    await page.getByText("Review together (2)").click();
    await page.getByText("Mark reviewed").click();
    await page.getByRole("button", { name: /tests\/accounts.rs/ }).click();
    await page.getByText("Mark reviewed").click();
    await page.reload();
    await open(url, "src/accounts.rs");
    await page.getByText("Tests · Reviewed", { exact: true }).waitFor();
    files[2].sha = "new";
    files[2].patch += "\n+// changed";
    await page.getByText("Open / refresh").click();
    const group = page.locator("section").filter({ hasText: "Account deletion · 2" });
    await group.getByText("Tests · Changed", { exact: true }).waitFor();
    await group.getByText("Implementation · Reviewed", { exact: true }).waitFor();
    console.log("Browser review workflow passed");
  } finally {
    await browser.close();
  }
}
run().catch(error => { console.error(error); process.exitCode = 1; });
