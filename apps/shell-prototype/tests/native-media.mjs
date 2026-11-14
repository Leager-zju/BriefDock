// Explicit local synthetic media only. No account, upstream URL or production instance.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile, rm, realpath } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import http from 'node:http';
import net from 'node:net';
import { chromium } from 'playwright-core';
const here=path.dirname(fileURLToPath(import.meta.url));
const root=path.resolve(here,'../../..');
const exe=path.resolve(process.argv[2]??path.join(root,'target/qa-native-video/build/debug/lazybuddy-shell-prototype.exe'));
const output=path.join(root,'target/verification-native-video');await mkdir(output,{recursive:true});
const scratch=path.join(root,'target/qa-native-video/profile');
const clip=await readFile(process.argv[3]??path.join(root,'target/shell-prototype/sample.mp4'));
let requests=0;
const server=http.createServer((req,res)=>{
  if(req.url!=='/sample.mp4'){res.writeHead(404).end();return;} requests++;
  let start=0,end=clip.length-1;
  const range=/^bytes=(\d+)-(\d*)$/.exec(req.headers.range??'');
  if(range){start=Number(range[1]);if(range[2])end=Math.min(end,Number(range[2]));}
  if(start>end||start>=clip.length){res.writeHead(416,{'Content-Range':`bytes */${clip.length}`}).end();return;}
  res.writeHead(range?206:200,{'Content-Type':'video/mp4','Accept-Ranges':'bytes','Content-Length':end-start+1,...(range?{'Content-Range':`bytes ${start}-${end}/${clip.length}`}:{})});
  res.end(req.method==='HEAD'?undefined:clip.subarray(start,end+1));
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const socket=net.createServer();await new Promise(resolve=>socket.listen(0,'127.0.0.1',resolve));const port=socket.address().port;await new Promise(resolve=>socket.close(resolve));
const child=spawn(exe,[],{env:{...process.env,LAZYBUDDY_PROTOTYPE_PROBE:'1',LAZYBUDDY_NATIVE_FIXTURE:'1',LAZYBUDDY_NATIVE_FIXTURE_URL:`http://127.0.0.1:${server.address().port}/sample.mp4`,LAZYBUDDY_NATIVE_PACKAGES:process.env.LAZYBUDDY_NATIVE_PACKAGES??path.join(root,'target/service-exports/lazybuddy-hongguo-service/target/packages-current'),WEBVIEW2_USER_DATA_FOLDER:scratch,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`},windowsHide:true,stdio:['ignore','pipe','pipe']});
let diagnostics='';child.stderr.on('data',data=>{diagnostics+=data;});
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){if(child.exitCode!==null)throw Error(`prototype exited ${child.exitCode}: ${diagnostics}`);if(await fn())return;await delay(80);}throw Error(`native media condition timeout: ${diagnostics}`);}
let browser,page,report,closed=false;
const invoke=(command,args={})=>page.evaluate(([command,args])=>window.__TAURI_INTERNALS__.invoke(command,args),[command,args]);
const snapshot=()=>invoke('native_snapshot');
try {
  await until(async()=>{try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},45000);
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await until(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(item=>item.url().includes('tauri.localhost'));return !!page;});
  page.on('pageerror',error=>{diagnostics+=String(error);});
  await page.waitForSelector('[data-events-ready=true]');
  await invoke('window_action',{action:'show'});
  assert.equal((await invoke('snapshot')).media_route,'native_d3d12');
  await page.getByLabel('选择功能').selectOption('hongguo');
  await page.getByRole('button',{name:'打开播放页',exact:true}).click();
  await until(async()=>{const state=await snapshot();if(state.error)throw Error(state.error);return state.frames>10&&state.position>.3;});
  assert.equal(await page.locator('video').count(),0);
  const initial=await snapshot();assert.ok(initial.width>0&&initial.height>0);
  await page.getByRole('button',{name:'暂停',exact:true}).click();
  await delay(200);const paused=await snapshot();await delay(350);const frozen=await snapshot();
  assert.equal(frozen.frames,paused.frames);assert.ok(Math.abs(frozen.position-paused.position)<.1);
  const pixels=[];
  for(const value of [0,.5,1]){await invoke('native_control',{method:'opacity',value});pixels.push(await invoke('native_probe'));}
  assert.equal(pixels[0][3],0);assert.ok(pixels[1][3]>=126&&pixels[1][3]<=129);assert.equal(pixels[2][3],255);
  for(let channel=0;channel<3;channel++)assert.ok(Math.abs(pixels[1][channel]*2-pixels[2][channel])<=2);
  execFileSync('python',['-B',path.join(here,'native-capture.py'),'--pid',String(child.pid),'--output',path.join(output,'composed.png')],{encoding:'utf8',windowsHide:true});
  const beforeSeek=(await snapshot()).frames;
  await invoke('native_control',{method:'seek',value:3});
  await until(async()=>{const state=await snapshot();return state.frames>beforeSeek&&!state.seeking&&state.position>=3&&state.position<4;});
  const seek=await snapshot();
  assert.equal(seek.playing,false);
  const seekPixel=await invoke('native_probe');
  if(process.argv.includes('--phases')) {
    assert.ok(pixels[2][0]>240&&pixels[2][1]<5&&pixels[2][2]<5,'initial phase must be red');
    assert.ok(seekPixel[0]<5&&seekPixel[1]>240&&seekPixel[2]<5,'paused seek to 3s must show the green phase');
  }
  await invoke('resize',{width:520,height:480});await delay(300);
  assert.equal((await snapshot()).decoder_pid,initial.decoder_pid);assert.equal((await snapshot()).source_opens,initial.source_opens);
  await page.getByLabel('倍速').selectOption('1.5');
  await invoke('native_control',{method:'play'});await delay(600);
  const accelerated=await snapshot();assert.equal(accelerated.speed,1.5);assert.ok(accelerated.position>seek.position+.4);
  const dock=JSON.parse(execFileSync('python',['-B',path.join(here,'native-dock.py'),'--pid',String(child.pid),'--action','drag-left'],{encoding:'utf8',windowsHide:true}));
  await page.waitForSelector('[data-docked=true]');await delay(200);const hidden=await snapshot();await delay(350);
  assert.equal((await snapshot()).frames,hidden.frames);assert.equal(hidden.playing,false);
  await page.getByRole('button',{name:'展开 LazyBuddy'}).click();await page.waitForSelector('[data-docked=false]');
  assert.equal((await snapshot()).playing,false);
  assert.equal((await snapshot()).decoder_pid,initial.decoder_pid);
  // Native capture includes the DirectComposition underlay; CDP screenshots do not.
  execFileSync('python',['-B',path.join(here,'native-capture.py'),'--pid',String(child.pid),'--output',path.join(output,'composed-resized.png')],{encoding:'utf8',windowsHide:true});
  await page.getByRole('button',{name:'设置',exact:true}).click();await until(async()=>!(await snapshot()).loaded);
  const released=await snapshot();assert.equal(released.decoder_pid,null);
  const count=execFileSync('powershell',['-NoProfile','-Command',`@(Get-Process -Id ${initial.decoder_pid} -ErrorAction SilentlyContinue).Count`],{encoding:'utf8',windowsHide:true});
  assert.equal(Number(count.trim()),0);
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await until(async()=>(await snapshot()).loaded);assert.equal((await snapshot()).playing,false);
  const restored=await snapshot();assert.ok(restored.session>initial.session);
  await page.getByRole('button',{name:'播放样例',exact:true}).click();
  await until(async()=>(await snapshot()).eos,15000);const ended=await snapshot();assert.equal(ended.playing,false);
  await page.getByRole('button',{name:'播放样例',exact:true}).click();
  await until(async()=>{const value=await snapshot();return value.frames>ended.frames+3&&value.position<2;});
  report={initial,paused,pixels,seek,seekPixel,accelerated,hidden,released,restored,ended,dock,requests,diagnostics,scope:'local synthetic D3D12 with paused preroll; no account or real programme'};
  await invoke('window_action',{action:'close'});closed=true;
  await new Promise((resolve,reject)=>{if(child.exitCode!==null)return resolve();const timer=setTimeout(()=>reject(Error('owned prototype did not exit')),15000);child.once('exit',()=>{clearTimeout(timer);resolve();});});
  await writeFile(path.join(output,'native-media.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify(report,null,2));
} catch(error) {
  if(page) { console.error('last native state',await snapshot()); await page.screenshot({path:path.join(output,'failure-overlay.png')}).catch(()=>{}); }
  throw error;
} finally {
  if(child.exitCode===null){if(page&&!closed)await invoke('window_action',{action:'close'}).catch(()=>{});await delay(1000);if(child.exitCode===null)child.kill();}
  await browser?.close().catch(()=>{});await new Promise(resolve=>server.close(resolve));
  await writeFile(path.join(output,'diagnostics.log'),diagnostics);
  if(child.exitCode!==null){const resolved=await realpath(scratch);assert.ok(resolved.startsWith(root+path.sep));await rm(resolved,{recursive:true,force:true,maxRetries:6,retryDelay:300});}
}
