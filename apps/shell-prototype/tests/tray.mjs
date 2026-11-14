// Close, native WM_CLOSE, actual tray callback restoration and explicit exit only.
import assert from 'node:assert/strict';
import {spawn,execFileSync} from 'node:child_process';
import {mkdir,realpath,rm,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import net from 'node:net';
import {chromium} from 'playwright-core';
const here=path.dirname(fileURLToPath(import.meta.url));const root=path.resolve(here,'../../..');
assert.ok(process.argv[2],'explicit candidate executable required');const exe=path.resolve(process.argv[2]);
const output=path.join(root,'target/verification-shell');await mkdir(output,{recursive:true});
const profile=path.join(root,'target/qa-browser-video/tray-profile');
const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));
const env={...process.env,LAZYBUDDY_PROTOTYPE_PROBE:'1',WEBVIEW2_USER_DATA_FOLDER:profile,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
delete env.LAZYBUDDY_BROWSER_PACKAGES;
const child=spawn(exe,[],{env,windowsHide:true,stdio:['ignore','ignore','pipe']});
let stderr='';child.stderr.on('data',chunk=>{stderr=(stderr+String(chunk)).slice(-8192);});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,timeout=25000){const end=Date.now()+timeout;while(Date.now()<end){if(child.exitCode!==null)throw Error('owned preview exited');if(await fn())return;await delay(80);}throw Error('tray condition timeout');}
let browser,page;
const invoke=(command,args={})=>page.evaluate(([command,args])=>window.__TAURI_INTERNALS__.invoke(command,args),[command,args]);
const native=action=>execFileSync('python',['-B',path.join(here,'native-tray.py'),'--pid',String(child.pid),'--exe',exe,'--action',action],{windowsHide:true,encoding:'utf8'});
try{
  await until(async()=>{try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}});
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await until(()=>{page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));return !!page;});
  await page.waitForSelector('[data-events-ready=true]');await invoke('window_action',{action:'show'});
  assert.equal((await invoke('snapshot')).tray,true);
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:'关闭到托盘',exact:true}).click();
  await until(async()=>!(await invoke('snapshot')).view.visible);
  await until(()=>!JSON.parse(native('snapshot')).visible);
  const before=await invoke('snapshot');await delay(1200);
  assert.equal((await invoke('snapshot')).feed_requests,before.feed_requests);
  native('restore');await until(async()=>(await invoke('snapshot')).view.visible);
  await until(()=>JSON.parse(native('snapshot')).visible);
  assert.equal((await invoke('snapshot')).view.section,'settings');
  native('close');await until(async()=>!(await invoke('snapshot')).view.visible);
  await until(()=>!JSON.parse(native('snapshot')).visible);
  assert.equal(child.exitCode,null);
  assert.doesNotMatch(stderr,/Failed to unregister class Chrome_WidgetWin_0/);
  native('restore');await until(async()=>(await invoke('snapshot')).view.visible);
  await page.getByRole('button',{name:'收起',exact:true}).click();
  await until(()=>JSON.parse(native('snapshot')).minimized);
  native('restore');await until(()=>!JSON.parse(native('snapshot')).minimized);
  await until(async()=>(await invoke('snapshot')).view.visible);
  await invoke('window_action',{action:'quit'});
  await Promise.race([new Promise(resolve=>child.once('exit',resolve)),delay(15000)]);
  assert.notEqual(child.exitCode,null,'explicit quit must exit normally');assert.equal(child.exitCode,0);
  const report={toolbar_close_hides:true,native_close_hides:true,tray_callback_restores:true,minimize_and_restore:true,settings_preserved:true,hidden_feed_requests_unchanged:true,no_close_unregister_warning:true,explicit_quit_exit:child.exitCode,scope:'owned offline preview; no accounts, media or physical input'};
  await writeFile(path.join(output,'tray.json'),JSON.stringify(report,null,2));console.log(report);
}catch(error){
  if(page){console.error({notice:await page.locator('.notice').textContent(),snapshot:await invoke('snapshot'),native:JSON.parse(native('snapshot'))});}
  throw error;
}finally{
  if(child.exitCode===null){if(page)await invoke('window_action',{action:'quit'}).catch(()=>{});await delay(1000);if(child.exitCode===null)child.kill();}
  await browser?.close().catch(()=>{});
  const resolved=await realpath(profile).catch(()=>null);if(resolved){assert.ok(resolved.startsWith(root+path.sep));await rm(resolved,{recursive:true,force:true,maxRetries:6,retryDelay:300});}
}
