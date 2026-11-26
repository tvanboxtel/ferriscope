// Linux WebKitGTK smoke test, Node 22+, a display, and optionally a real server.
// See README. The inspector is enabled only for this isolated test process.
import assert from "node:assert/strict";
import {execFileSync, spawn} from "node:child_process";
import {mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync} from "node:fs";
import {createServer} from "node:net";
import {tmpdir} from "node:os";
import {join, resolve} from "node:path";
import {setTimeout as delay} from "node:timers/promises";

const binary = resolve("target/debug/ferriscope");
const dir = mkdtempSync(join(tmpdir(), "ferriscope-definition-"));
const root = join(dir, "checkout"), bin = join(dir, "bin");
const fixture = name => readFileSync(new URL(name, import.meta.url), "utf8");
let app, socket;
try {
  for (const path of [root, bin]) mkdirSync(path);
  const server=join(bin,"typescript-language-server");
  const peer=process.env.FERRISCOPE_TEST_LSP || join(bin,"fixture-lsp");
  writeFileSync(server,'#!/bin/sh\necho start >> "$HOME/starts"\nexec "$TEST_PEER" "$@"\n',{mode:0o755});
  writeFileSync(join(dir,"starts"),"");
  const starts=()=>readFileSync(join(dir,"starts"),"utf8").split("\n").filter(Boolean).length;
  if (!process.env.FERRISCOPE_TEST_LSP) {
    writeFileSync(peer,fixture("fake-lsp.py"),{mode:0o755});
    writeFileSync(join(bin,"mode"),"utf-8");
  }
  const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=test@example.invalid",
    "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", ...args], {cwd:root, encoding:"utf8"}).trim();
  git("init", "-q");
  for (const name of ["tsconfig.json", "library.ts"]) writeFileSync(join(root,name),fixture(name));
  const lines = fixture("caller.ts").trimEnd().split("\n");
  const removed = "export const old = 1;";
  writeFileSync(join(root, "caller.ts"), lines[0]+"\n"+removed+"\n");
  git("add", "."); git("commit", "-qm", "base");
  lines.splice(2, 0, ...Array(120).fill("// unchanged context above this call"));
  lines.push(...Array(90).fill("// more code below"));
  writeFileSync(join(root, "caller.ts"), lines.join("\n") + "\n");
  git("add", "."); git("commit", "-qm", "add caller");
  const meta = {base:{sha:git("rev-parse","HEAD^")},head:{sha:git("rev-parse","HEAD")},changed_files:1};
  const caller = {filename:"caller.ts",status:"modified",sha:"fixture",patch:`@@ -1,2 +1,${lines.length} @@\n ${lines[0]}\n-${removed}\n`+lines.slice(1).map(l=>"+"+l).join("\n")};
  const files = [...Array.from({length:22},(_,i)=>({filename:`docs/${i}.md`,status:"added",sha:`fixture${i}`})),caller];
  files.push({filename:"widget.tsx",status:"added",sha:"tsx",patch:"@@ -0,0 +1,250 @@\n"+
    Array.from({length:250},(_,i)=>`+export const Row${i} = ({name}: Props) => <span title={name}>{name}</span>;`).join("\n")});
  meta.changed_files=files.length;
  git("checkout","-qb","my-work",meta.base.sha);
  writeFileSync(join(root,"library.ts"),"// my uncommitted work\n");
  const localStatus=git("status","--porcelain");
  // A newer PR head exists only on the simulated remote. Redirect this app
  // process's GitHub URL there to exercise opt-in fetch without network access.
  const remote=join(dir,"upstream");
  execFileSync("git",["clone","-q","--no-local",root,remote]);
  const upstream=(...args)=>execFileSync("git",["-C",remote,"-c","user.name=Fixture","-c","user.email=test@example.invalid","-c","core.hooksPath=/dev/null","-c","commit.gpgsign=false",...args],{encoding:"utf8"}).trim();
  upstream("checkout","-q","--detach",meta.head.sha);
  upstream("commit","--allow-empty","-qm","remote only");
  const remoteHead=upstream("rev-parse","HEAD");
  upstream("update-ref","refs/pull/1/head",remoteHead);
  writeFileSync(join(bin,"gh"), `#!/bin/sh
case "$*" in
  *review-requested:@me*) echo '{"total_count":1,"incomplete_results":false,"items":[{"title":"Fixture PR","html_url":"https://github.com/demo/test/pull/1","draft":false}]}' ;;
  *assignee:@me*) echo '{"total_count":0,"incomplete_results":false,"items":[]}' ;;
  *repos/demo/test/pulls/1/files*) cat "$HOME/files.json" ;;
  *repos/demo/test/pulls/1) cat "$HOME/meta.json" ;;
  *) exit 1 ;;
esac\n`,{mode:0o755});
  writeFileSync(join(dir,"meta.json"),JSON.stringify(meta));
  writeFileSync(join(dir,"files.json"),JSON.stringify(files));
  const reservation=createServer();
  await new Promise(r=>reservation.listen(0,"127.0.0.1",r));
  const address="127.0.0.1:"+reservation.address().port;
  await new Promise(r=>reservation.close(r));
  const trace=join(dir,"git-trace");
  writeFileSync(trace,"");
  const fetches=()=>readFileSync(trace,"utf8").split("\n").filter(l=>l.includes('"event":"start"') && l.includes('"fetch"')).length;
  const launch=()=>spawn(binary,[],{env:{...process.env,PATH:bin+":"+process.env.PATH,HOME:dir,TEST_PEER:peer,GIT_TRACE2_EVENT:trace,
    XDG_DATA_HOME:join(dir,"data"),XDG_CACHE_HOME:join(dir,"cache"),XDG_CONFIG_HOME:join(dir,"config"),
    GIT_CONFIG_COUNT:"1",GIT_CONFIG_KEY_0:`url.file://${remote}.insteadOf`,GIT_CONFIG_VALUE_0:"https://github.com/demo/test.git",
    GDK_BACKEND:"x11",WEBKIT_INSPECTOR_HTTP_SERVER:address},stdio:"ignore"});
  async function until(fn, description) {
    for(let i=0;i<200;i++){const result=await fn(); if(result)return result; await delay(100);}
    throw Error("Timed out: "+description);
  }
  let target,seq=0;
  const pending=new Map();
  async function start(){
  app=launch();target=null;
  const endpoint=await until(async()=>{try{
    return (await (await fetch("http://"+address)).text()).match(/\/socket\/\d+\/\d+\/WebPage/)?.[0];
  }catch{return false;}}, "desktop WebKit target");
  socket=new WebSocket("ws://"+address+endpoint);
  socket.onmessage=e=>{
    const msg=JSON.parse(e.data);
    if(msg.method==="Target.targetCreated") target=msg.params.targetInfo.targetId;
    if(msg.method==="Target.dispatchMessageFromTarget"){
      const inner=JSON.parse(msg.params.message);
      pending.get(inner.id)?.(inner); pending.delete(inner.id);
    }
  };
  await until(()=>target,"page target");
  }
  await start();
  async function evaluate(expression){
    const id=++seq;
    const response=new Promise(resolve=>pending.set(id,resolve));
    socket.send(JSON.stringify({id,method:"Target.sendMessageToTarget",params:{targetId:target,
      message:JSON.stringify({id,method:"Runtime.evaluate",params:{expression,returnByValue:true}})}}));
    const msg=await response;
    assert(!msg.error && !msg.result.wasThrown,JSON.stringify(msg));
    return msg.result.result.value;
  }
  const wait=expr=>until(()=>evaluate(expr),expr);
  const button=text=>`Array.from(document.querySelectorAll("button")).find(b=>b.textContent.includes(${JSON.stringify(text)}) && b.offsetWidth)`;
  async function click(text){const b=button(text);await wait(`!!(${b})`);await evaluate(`${b}.click()`);await delay(80);}
  async function enter(selector,value){await evaluate(`(()=>{const n=document.querySelector(${JSON.stringify(selector)});n.value=${JSON.stringify(value)};n.dispatchEvent(new Event('input',{bubbles:true}));})()`);await delay(80);}
  const windowId=execFileSync("xdotool",["search","--sync","--pid",String(app.pid),"--onlyvisible","--name","Ferriscope"],{encoding:"utf8",timeout:5000}).trim().split("\n")[0];
  execFileSync("xdotool",["windowsize","--sync",windowId,"1000","820"]);
  await click("Fixture PR");await click("Raw review");await click("Next");
  assert.equal(await evaluate(`!!(${button("Go to definition")})`), false);
  const menu=()=>evaluate("document.querySelector('header .definition-onboarding > summary').click()");
  await menu();
  await enter(".definition-setup label:nth-of-type(1) input",root);
  assert.equal(await evaluate("document.querySelector('input[placeholder=\"owner/repository\"]').value"),"demo/test");
  if(process.env.FERRISCOPE_TEST_LSP) await enter(".definition-setup details input",server);
  else assert.equal(await evaluate("document.querySelector('.definition-setup details input').value"),server);
  await click("Trust and connect");await wait(`!!(${button("Disconnect checkout")})`);
  await wait("document.querySelector('.source-timings ul')?.textContent.match(/Verifying checkout: [0-9.]+s/)");
  await menu();
  // Time ordinary file switches until the new diff text is painted, not the
  // inspector polling delay or the later syntax-color pass.
  async function switchTime(direction,path,sample) {
    await evaluate(`(()=>{const start=performance.now(),pane=document.querySelector('.reader');delete document.body.dataset.switchMs;
      const observer=new MutationObserver(()=>{if(pane.querySelector('h2')?.textContent!==${JSON.stringify(path)} ||
        !pane.querySelector('.diff-scroll pre')?.textContent.includes(${JSON.stringify(sample)}))return;
        observer.disconnect();requestAnimationFrame(()=>requestAnimationFrame(()=>document.body.dataset.switchMs=performance.now()-start));});
      observer.observe(pane,{subtree:true,childList:true,characterData:true});${button(direction)}.click()})()`);
    await wait("document.body.dataset.switchMs !== undefined");
    return Math.round(await evaluate("Number(document.body.dataset.switchMs)"));
  }
  const initialStarts=starts(),initialFetches=fetches();
  const firstSwitch=await switchTime("Next","widget.tsx","Row200");
  await wait("!!document.querySelector('.diff-scroll .add span[style]')");
  const previousSwitch=await switchTime("Previous","caller.ts","alias()");
  const cachedSwitch=await switchTime("Next","widget.tsx","Row200");
  console.log(`File switch to paint: first TSX ${firstSwitch} ms; previous ${previousSwitch} ms; cached TSX ${cachedSwitch} ms`);
  await switchTime("Previous","caller.ts","alias()");
  assert.equal(starts(),initialStarts);assert.equal(fetches(),initialFetches);
  await click("Mark reviewed");
  assert(await evaluate(`document.querySelector('[data-definition-line="1"]').textContent.startsWith('import')`));
  assert.equal(await evaluate(`document.querySelector('.del').querySelector('[data-definition-line]') === null`),true);
  const row=`document.querySelector('[data-definition-line="123"]')`;
  await evaluate(`${row}.scrollIntoView({block:'center'})`);
  await evaluate(`${row}.parentElement.querySelector(".line-comment").click()`);
  await wait("!!document.querySelector('textarea')");await enter("textarea","Unsaved draft");
  const scroll="document.querySelector('.diff-scroll').scrollTop", before=await evaluate(scroll);
  assert(before>0);
  async function clickAlias(ctrlKey=true, target=row, name="alias"){
    const [x,y]=await evaluate(`(()=>{const w=document.createTreeWalker(${target},NodeFilter.SHOW_TEXT);
    for(let n;n=w.nextNode();){const i=n.textContent.indexOf(${JSON.stringify(name)});if(i<0)continue;
    const r=new Range();r.setStart(n,i+2);r.setEnd(n,i+3);const b=r.getBoundingClientRect();
    return [Math.floor(b.x+b.width/2),Math.floor(b.y+b.height/2)];}
    throw Error("Alias not in rendered diff");})()`);
    await evaluate(`document.addEventListener('mousemove',e=>{document.body.dataset.pointer=JSON.stringify([e.clientX,e.clientY])},{once:true,capture:true})`);
    execFileSync("xdotool",["mousemove","--window",windowId,"100","100"]);
    await wait("!!document.body.dataset.pointer");
    const [ox,oy]=await evaluate("JSON.parse(document.body.dataset.pointer)");
    await evaluate("delete document.body.dataset.pointer");
    execFileSync("xdotool",["windowfocus","--sync",windowId,"mousemove","--window",windowId,String(x+100-ox),String(y+100-oy),
      ...(ctrlKey?["keydown","Control_L"]:[]),"click","1",...(ctrlKey?["keyup","Control_L"]:[])]);
  }
  await clickAlias(false);await delay(150);assert.equal(await evaluate("!!document.querySelector('.definition-line')"),false);
  await evaluate(`new MutationObserver(()=>{let s=document.querySelector('.file-nav .lookup-spinner'); if(s){let r=s.getBoundingClientRect();if(r.top>=0&&r.bottom<=innerHeight)document.body.dataset.sawProgress='yes'}}).observe(document.querySelector('.file-nav'),{childList:true,subtree:true})`);
  const cold=performance.now();await clickAlias();await wait("!!document.querySelector('.definition-line')");
  console.log(`First definition (including UI input): ${Math.round(performance.now()-cold)} ms`);
  assert.equal(await evaluate("document.body.dataset.sawProgress"),"yes");
  assert(await evaluate("new Set([...document.querySelectorAll('.definition-line span[style]')].map(n=>n.style.color)).size>1"));
  assert.equal(await evaluate("document.querySelector('.definition-view h2').textContent"),"library.ts");
  assert.match(await evaluate("document.querySelector('.definition-line').textContent"),/function implementation/);
  const warm=performance.now();await clickAlias(true,`document.querySelector('.definition-view [data-definition-line="3"]')`,"answer");
  await wait("document.querySelector('.definition-line')?.textContent.includes('function answer')");
  console.log(`Chained definition (including UI input): ${Math.round(performance.now()-warm)} ms`);
  await click("Previous definition");
  await wait("document.querySelector('.definition-line')?.textContent.includes('function implementation')");
  for(const [selector,visible] of [[".definition-view",true],["aside",true],[".review-tray",false],[".file-heading",false]]){
    assert.equal(await evaluate(`getComputedStyle(document.querySelector('${selector}')).visibility`),visible?"visible":"hidden");
  }
  await click("Back to diff");
  await wait("document.activeElement === document.querySelector('main')");
  assert.equal(await evaluate(scroll),before);
  assert.equal(await evaluate("document.querySelector('textarea').value"),"Unsaved draft");
  assert(await evaluate(`!!(${button("Mark unread")})`));
  assert.equal(starts(),1);
  await clickAlias();
  await wait("!!document.querySelector('.definition-line')");
  assert.equal(await evaluate("document.querySelector('.definition-onboarding').open"),false);
  await click("Back to diff");
  await wait("document.activeElement === document.querySelector('main')");
  assert.equal(starts(),1,"Second lookup should use the same server");
  assert.equal(git("branch","--show-current"),"my-work");
  assert.equal(git("status","--porcelain"),localStatus);
  assert.equal(readFileSync(join(root,"library.ts"),"utf8"),"// my uncommitted work\n");
  // Refresh while the group containing original index 22 is expanded.
  writeFileSync(join(dir,"files.json"),JSON.stringify([caller]));
  writeFileSync(join(dir,"meta.json"),JSON.stringify({...meta,changed_files:1,head:{sha:remoteHead}}));
  await click("Open / refresh");await wait("document.querySelector('aside p')?.textContent==='1 changed files'");
  await click("Next");await wait(`!!(${row})`);
  await evaluate(`${row}.scrollIntoView({block:'center'})`);await clickAlias();
  await wait("document.querySelector('.file-nav [role=status]')?.textContent.includes('not available locally')");
  assert.equal(fetches(),0,"No fetch without permission");
  assert.equal(await evaluate(`!!(${button("Fetch PR and retry")})`),false);
  await menu();
  assert.equal(await evaluate("document.querySelector('.definition-setup input[type=checkbox]').checked"),false);
  await evaluate("document.querySelector('.definition-setup input[type=checkbox]').click()");
  await wait("document.querySelector('.definition-setup [role=status]')?.textContent.includes('saved')");
  await menu();await evaluate(`${row}.scrollIntoView({block:'center'})`);
  await clickAlias();await wait("!!document.querySelector('.definition-line')");
  await click("Back to diff");
  assert.equal(git("status","--porcelain"),localStatus);
  assert.equal(git("rev-parse","HEAD"),meta.base.sha);
  assert.throws(()=>git("cat-file","-e",remoteHead));
  socket.close();const exited=new Promise(r=>app.once("exit",r));app.kill();await exited;await start();
  const previousFetches=fetches(),previousStarts=starts();
  await click("Fixture PR");await click("Raw review");
  await until(()=>fetches()>previousFetches,"background fetch on opening PR before selecting a file");
  assert.equal(starts(),previousStarts,"Opening the reader shouldn't launch the language server");
  await click("Next");await menu();
  assert.equal(await evaluate("document.querySelector('.definition-setup input[type=checkbox]').checked"),true);
  assert.equal(await evaluate("document.querySelector('.definition-setup').innerText.includes('Trust and connect')"),false);
  await evaluate("document.querySelectorAll('.definition-setup input[type=checkbox]')[1].click()");
  await until(()=>starts()===previousStarts+1,"opt-in indexing before Ctrl-click");
  await click("Disconnect checkout");
  await wait("!!document.querySelector('.definition-setup input[placeholder=\"Or enter an absolute path\"]')");
  console.log(`Passed (${server}): highlighted target, draft, review state and scroll round trip.`);
} finally {
  socket?.close();
  if(app && app.exitCode===null && app.signalCode===null) {
    const exited=new Promise(r=>app.once("exit",r)); app.kill(); await exited;
  }
  rmSync(dir,{recursive:true,force:true});
}
