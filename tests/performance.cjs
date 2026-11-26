// Use the same CHROME, APP_URL and NODE_PATH as tests/browser.cjs.
const { chromium } = require("playwright-core");
const assert = require("node:assert/strict");

(async () => {
  const browser = await chromium.launch({ executablePath: process.env.CHROME });
  try {
    const page = await browser.newPage();
    const files = Array.from({length: 600}, (_, i) => ({
      filename: `src/module_${i}.rs`, status: "added", sha: `blob${i}`,
      patch: i === 0 ? "@@ -0,0 +1,3000 @@\n" + Array.from({length:3000}, (_, n) => `+pub const VALUE_${n}: usize = ${n};`).join("\n") : "@@ -0,0 +1 @@\n+pub fn example() {}"
    }));
    await page.route("https://api.github.com/repos/demo/perf/**", async route => {
      const url = new URL(route.request().url());
      await new Promise(resolve => setTimeout(resolve, 100));
      const start = (Number(url.searchParams.get("page")) - 1) * 100;
      await route.fulfill({json: url.pathname.endsWith("/files") ? files.slice(start,start+100) :
        {head:{sha:"head"},base:{sha:"base"},changed_files:files.length}});
    });
    await page.goto(process.env.APP_URL || "http://127.0.0.1:8080");
    await page.getByLabel("GitHub PR URL").fill("https://github.com/demo/perf/pull/1");
    const start = performance.now();
    await page.getByText("Open / refresh").click();
    await page.getByRole("heading",{name:"demo/perf/pull/1"}).waitFor();
    const opened = performance.now();
    await page.keyboard.press("j");
    await page.locator("pre .add").first().waitFor();
    const shown = performance.now();
    const rows = await page.locator("pre .add").count();
    await page.keyboard.press("j");
    await page.getByRole("heading",{name:"src/module_1.rs",exact:true}).waitFor();
    console.log(`Open: ${Math.round(opened-start)} ms; first large diff: ${Math.round(shown-opened)} ms (${rows} added rows mounted); next file: ${Math.round(performance.now()-shown)} ms`);
    if (process.env.CHECK_BOUNDS) {
      assert.ok(rows <= 250);
      assert.ok(await page.getByRole("checkbox").count() <= 60);
      const search=page.getByRole("searchbox",{name:"Search files"});
      await search.fill("module_599.rs");
      await search.press("Enter");
      await page.getByRole("heading",{name:"src/module_599.rs",exact:true}).waitFor();
      await page.reload();
      await page.getByLabel("GitHub PR URL").fill("https://github.com/demo/perf/pull/1");
      await page.getByText("Open / refresh").click();
      await page.getByRole("heading",{name:"demo/perf/pull/1"}).waitFor();
      await page.keyboard.press("j");
      await page.keyboard.press("j");
      await page.getByRole("button",{name:"Show more files"}).click();
      await page.getByLabel("Select src/module_119.rs",{exact:true}).waitFor();
      await page.locator("main").focus();
      await page.keyboard.press("k");
      await page.getByRole("button",{name:"Show more lines"}).click();
      await page.getByText("VALUE_300",{exact:true}).waitFor();
    }
  } finally { await browser.close(); }
})().catch(e => {console.error(e);process.exitCode=1;});
