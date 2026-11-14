// Compressed media from an external service package; no accounts/native decoder.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,writeFile,realpath,rm} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import net from 'node:net';
import {chromium} from 'playwright-core';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const output=path.join(root,'target/verification-browser-video');await mkdir(output,{recursive:true});
const profile=path.join(root,'target/qa-browser-video/profile');
assert.ok(process.env.LAZYBUDDY_BROWSER_PACKAGES,'explicit external fixture package required');
const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));
assert.ok(process.argv[2],'explicit candidate executable required');
const exe=path.resolve(process.argv[2]);
const child=spawn(exe,[],{windowsHide:true,stdio:'ignore',env:{...process.env,LAZYBUDDY_PROTOTYPE_PROBE:'1',WEBVIEW2_USER_DATA_FOLDER:profile,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,timeout=25000){const end=Date.now()+timeout;while(Date.now()<end){if(child.exitCode!==null)throw Error('owned prototype exited');if(await fn())return;await delay(80);}throw Error('browser media timeout');}
let browser,page,report;
const invoke=(command,args={})=>page.evaluate(([command,args])=>window.__TAURI_INTERNALS__.invoke(command,args),[command,args]);
try{
  await until(async()=>{try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}});
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await until(()=>{page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));return !!page;});
  await page.waitForSelector('[data-events-ready=true]');await invoke('window_action',{action:'show'});
  assert.equal((await invoke('snapshot')).media_route,'browser_service');
  await page.getByLabel('选择功能').selectOption('hongguo');await page.getByRole('button',{name:'打开播放页',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('video')?.currentTime>.3);
  const read=()=>page.locator('video').evaluate(v=>({position:v.currentTime,width:v.videoWidth,height:v.videoHeight,paused:v.paused,muted:v.muted,rate:v.playbackRate,error:v.error?.code??null}));
  const initial=await read();assert.ok(initial.width>0&&initial.height>0);assert.equal(initial.error,null);
  await page.getByRole('button',{name:'暂停',exact:true}).click();await delay(150);
  await page.locator('video').evaluate(v=>v.currentTime=3);
  await page.waitForFunction(()=>{const v=document.querySelector('video');return !v.seeking&&v.currentTime>=3;});
  const mediaTime=await page.locator('video').evaluate(v=>new Promise(resolve=>{v.requestVideoFrameCallback((_,m)=>resolve(m.mediaTime));v.currentTime=3.1;}));
  assert.ok(Math.abs(mediaTime-3.1)<.15);const seek=await read();assert.equal(seek.paused,true);assert.equal(seek.muted,initial.muted);
  const privateUrl=await page.locator('video').getAttribute('src');
  await invoke('window_action',{action:'hide'});await until(async()=>(await invoke('snapshot')).view.visible===false);
  await page.waitForFunction(()=>!document.querySelector('video')?.getAttribute('src'));
  await until(async()=>(await fetch(privateUrl,{headers:{Origin:'http://tauri.localhost'}})).status===409);
  await invoke('window_action',{action:'show'});await page.waitForFunction(()=>document.querySelector('video')?.readyState>=2);
  const restored=await read();assert.equal(restored.paused,true);assert.ok(Math.abs(restored.position-seek.position)<.2);
  assert.equal(await page.locator('video').getAttribute('src'),privateUrl);
  await page.screenshot({path:path.join(output,'browser-player.png')});
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await until(async()=>{try{await fetch(privateUrl,{headers:{Origin:'http://tauri.localhost'}});return false;}catch{return true;}});
  report={initial,seek,mediaTime,restored,scope:'synthetic external browser resource; no accounts; no GStreamer decoder'};
  await invoke('window_action',{action:'close'});
  await new Promise((resolve,reject)=>{if(child.exitCode!==null)return resolve();const timer=setTimeout(()=>reject(Error('prototype exit timeout')),15000);child.once('exit',()=>{clearTimeout(timer);resolve();});});
  await writeFile(path.join(output,'browser-media.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
}catch(error){if(page){console.error(await page.locator('.notice').textContent());await page.screenshot({path:path.join(output,'failure.png')}).catch(()=>{});}throw error;}
finally{
  if(child.exitCode===null){if(page)await invoke('window_action',{action:'close'}).catch(()=>{});await delay(1000);if(child.exitCode===null)child.kill();}
  await browser?.close().catch(()=>{});
  const resolved=await realpath(profile).catch(()=>null);if(resolved){assert.ok(resolved.startsWith(root+path.sep));await rm(resolved,{recursive:true,force:true,maxRetries:6,retryDelay:300});}
}
