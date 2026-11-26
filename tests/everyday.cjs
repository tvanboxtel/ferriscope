// See README for CHROME, APP_URL and NODE_PATH. One fresh tab per trial.
const {chromium} = require("playwright-core");
const assert = require("node:assert/strict");
const files = structuredClone(require("./mixed.json"));
files[1].patch = "@@ -0,0 +1,24 @@\n" + Array.from({length:24}, (_, i) =>
  `+pub const VALUE_${i}: usize = ${i};`).join("\n");
const browserOptions = {executablePath: process.env.CHROME};
async function measure(browser) {
  const page = await browser.newPage();
  try {
    let requests = 0, pages = 0, base = "base";
    await page.route("https://api.github.com/repos/demo/small/**", async r => {
      requests++;
      const isPage = r.request().url().includes("/files?");
      if (isPage) pages++;
      await new Promise(resolve => setTimeout(resolve,100));
      await r.fulfill({json:isPage ? files :
        {head:{sha:"head"},base:{sha:base},changed_files:files.length}});
    });
    await page.goto(process.env.APP_URL || "http://127.0.0.1:8080");
    await page.getByLabel("GitHub PR URL").fill("https://github.com/demo/small/pull/1");
    // Observe DOM changes in-page: locator polling/auto-wait adds substantial
    // noise when the actual operation takes only a few milliseconds.
    async function timed(action, selector) {
      await page.evaluate(selector => {
        window.elapsed = new Promise(resolve => {
          const start = performance.now();
          const observer = new MutationObserver(() => {
            if (!document.querySelector(selector)) return;
            observer.disconnect(); resolve(performance.now()-start);
          });
          observer.observe(document.body,{childList:true,subtree:true,attributes:true});
        });
      },selector);
      await action();
      await page.waitForSelector(selector,{timeout:10000});
      return Math.round(await page.evaluate(() => window.elapsed));
    }
    const submit = () => page.locator("header form").evaluate(form => form.requestSubmit());
    const open = await timed(submit,"main aside");
    const diff = await timed(() => page.keyboard.press("j"),"article pre .add span[style]");
    assert.equal(await page.locator("article pre .add").count(),24);
    const total = requests;
    const refresh = await timed(submit,"article .empty");
    const refreshRequests = requests - total;
    if(process.env.CHECK_CACHE) {
      assert.equal(refreshRequests,1);
      const before = pages;
      base = "new-base"; // unchanged head is not enough to reuse patches
      await submit();
      await page.getByRole("button",{name:"Open / refresh"}).waitFor();
      assert.equal(pages,before+1);
      // A different PR can have identical commit coordinates. Never let it
      // inherit files (or access) from the current PR.
      await page.getByLabel("GitHub PR URL").fill("https://github.com/demo/small/pull/2");
      await submit();
      await page.getByRole("heading",{name:"demo/small/pull/2"}).waitFor();
      assert.equal(pages,before+2);
      await page.route("https://api.github.com/repos/demo/small/pulls/2", r =>
        r.fulfill({status:403,json:{message:"Forbidden"}}));
      await submit();
      await page.getByRole("alert").filter({hasText:"403"}).waitFor();
      assert.equal(pages,before+2);
    }
    return {open,diff,refresh,openRequests:total,refreshRequests};
  } finally {await page.close();}
}
(async()=>{
  const browser=await chromium.launch(browserOptions);
  try {
    const rows=[];
    for(let n=0;n<5;n++) rows.push(await measure(browser));
    console.table(rows);
    console.log("Median ms:",Object.fromEntries(["open","diff","refresh"].map(k=>
      [k,rows.map(r=>r[k]).sort((a,b)=>a-b)[2]])));
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
