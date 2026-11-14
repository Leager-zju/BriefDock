// New toolbar/navigation and native docking only; no full business/media regression.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, writeFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import net from 'node:net';
import { chromium } from 'playwright-core';
const here=path.dirname(fileURLToPath(import.meta.url));
const root=path.resolve(here,'../../..');
const exe=path.resolve(process.argv[2]??path.join(root,'target/qa-web-shell/build/debug/lazybuddy-shell-prototype.exe'));
const output=path.join(root,'target/verification-shell');await mkdir(output,{recursive:true});
const scratch=path.join(root,'target/qa-web-shell/dock-profile');await mkdir(scratch,{recursive:true});
const socket=net.createServer();await new Promise(resolve=>socket.listen(0,'127.0.0.1',resolve));const port=socket.address().port;await new Promise(resolve=>socket.close(resolve));
const env={...process.env,LAZYBUDDY_PROTOTYPE_PROBE:'1',WEBVIEW2_USER_DATA_FOLDER:scratch,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`};
if(!existsSync(path.join(path.dirname(exe),'sample.mp4')))env.LAZYBUDDY_PROTOTYPE_CLIP=path.join(root,'target/qa-web-shell/sample.mp4');
const child=spawn(exe,[],{env,windowsHide:true,stdio:'ignore'});
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){if(await fn())return;await delay(80);}throw Error('dock condition timeout');}
function native(action='snapshot'){return JSON.parse(execFileSync('python',['-B',path.join(here,'native-dock.py'),'--pid',String(child.pid),'--action',action],{encoding:'utf8',windowsHide:true}));}
let browser,page,report;
try{
  await until(async()=>{if(child.exitCode!==null)throw Error('prototype exited before startup');try{return (await fetch(`http://127.0.0.1:${port}/json/version`)).ok;}catch{return false;}},45000);
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  await until(()=>{page=browser.contexts().flatMap(context=>context.pages()).find(item=>item.url().includes('tauri.localhost'));return !!page;});
  const invoke=(command,args={})=>page.evaluate(([command,args])=>window.__TAURI_INTERNALS__.invoke(command,args),[command,args]);
  await page.waitForSelector('[data-testid=shell]');
  await page.waitForFunction(()=>document.querySelector('[data-testid=shell]').dataset.eventsReady==='true');
  await invoke('window_action',{action:'show'});
  await until(async()=>await page.locator('.source-card').count()===3);
  for(const label of ['功能','设置']){
    const button=page.getByRole('button',{name:label,exact:true});
    assert.equal((await button.textContent()).trim(),'');assert.equal(await button.getAttribute('title'),label);
  }
  await page.screenshot({path:path.join(output,'toolbar-icons.png')});
  await page.getByRole('button',{name:'功能',exact:true}).click();
  const title=await page.getByRole('heading',{name:'可用功能'}).boundingBox();
  const back=await page.getByRole('button',{name:'返回',exact:true}).boundingBox();
  assert.ok(back.x>title.x+title.width);assert.ok(Math.abs(back.y+back.height/2-title.y-title.height/2)<3);
  await page.screenshot({path:path.join(output,'modules-back.png')});
  await page.getByRole('button',{name:'返回',exact:true}).click();
  assert.equal(await page.locator('[data-testid=shell]').getAttribute('data-section'),'content');
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:'显示设置',exact:true}).click();
  await page.getByRole('button',{name:'功能',exact:true}).click();
  await page.getByRole('button',{name:'返回',exact:true}).click();
  assert.equal(await page.locator('[data-testid=shell]').getAttribute('data-section'),'content');
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('heading',{name:'显示设置',exact:true}).waitFor();
  await page.getByRole('button',{name:'切换深色',exact:true}).click();
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByLabel('选择功能').selectOption('hongguo');
  await page.getByRole('button',{name:'打开播放页',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('video')?.currentTime>.3);
  const requests=(await invoke('snapshot')).feed_requests;
  const left=native('drag-left');
  assert.equal(left.before_exit[2],left.before[2]); // The move itself must not collapse the window.
  await page.waitForFunction(()=>document.querySelector('[data-testid=shell]').dataset.docked==='true'&&innerWidth<100);
  const size=Math.round(40*left.dpi/96);assert.deepEqual(left.after.slice(2),[size,size]);
  assert.equal(left.region.corner,false);assert.equal(left.region.center,true);assert.equal(left.topmost,true);
  assert.equal(left.region.corner_hits_prototype,false);assert.equal(left.region.center_hits_prototype,true);
  assert.equal((await invoke('snapshot')).view.visible,false);
  await page.waitForFunction(()=>document.querySelector('video').paused);
  const frame=await page.locator('[data-testid=shell]').getAttribute('data-frames');
  const time=await page.locator('video').evaluate(video=>video.currentTime);
  await delay(300);assert.equal(await page.locator('[data-testid=shell]').getAttribute('data-frames'),frame);
  assert.ok(Math.abs(await page.locator('video').evaluate(video=>video.currentTime)-time)<.1);
  assert.equal((await invoke('snapshot')).feed_requests,requests);
  assert.equal(await page.locator('.toolbar').isVisible(),false);
  assert.equal(await page.locator('.info-bar').isVisible(),false);
  assert.equal((await page.getByRole('button',{name:'展开 LazyBuddy'}).textContent()).trim(),'');
  await page.screenshot({path:path.join(output,'dock-logo-left.png')});
  await page.getByRole('button',{name:'展开 LazyBuddy'}).click();
  await page.waitForFunction(()=>document.querySelector('[data-testid=shell]').dataset.docked==='false'&&innerWidth>400);
  const restored=native();assert.deepEqual(restored.after.slice(2),left.before.slice(2));
  assert.equal(restored.topmost,false);assert.equal(await page.locator('video').evaluate(video=>video.paused),true);
  const right=native('drag-right');await page.getByRole('button',{name:'展开 LazyBuddy'}).waitFor();
  assert.deepEqual(right.after.slice(2),[size,size]);assert.equal(right.region.corner,false);
  await page.screenshot({path:path.join(output,'dock-logo-right.png')});
  await page.getByRole('button',{name:'展开 LazyBuddy'}).click();
  await page.waitForFunction(()=>innerWidth>400);
  const centered=native('drag-center');assert.equal(centered.after[2],centered.before[2]);
  assert.equal((await invoke('snapshot')).desktop.docked,false);
  const resized=native('resize-left');assert.equal(resized.after[2],resized.before[2]-4);
  assert.equal((await invoke('snapshot')).desktop.docked,false);
  await page.getByLabel('选择功能').selectOption('news');
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByRole('button',{name:'新闻设置',exact:true}).click();
  await page.locator('.page').evaluate(page=>page.scrollTop=150);
  const scroll=await page.locator('.page').evaluate(page=>page.scrollTop);assert.ok(scroll>100);
  native('drag-left');await page.getByRole('button',{name:'展开 LazyBuddy'}).waitFor();
  await page.getByRole('button',{name:'展开 LazyBuddy'}).click();
  await page.waitForFunction(()=>document.querySelector('[data-testid=shell]').dataset.docked==='false'&&document.querySelector('.page').scrollTop>100);
  assert.ok(Math.abs(await page.locator('.page').evaluate(page=>page.scrollTop)-scroll)<2);
  assert.equal(await page.locator('[data-testid=shell]').getAttribute('data-section'),'settings');
  report={passed:true,icon_only_toolbar:true,module_back_same_row:true,module_back_restores_settings:true,left_and_right_native_move_end:true,not_collapsed_before_move_end:true,circle_logical_pixels:40,native_ellipse_corner_excluded:true,logo_click_restores_size:true,scroll_and_settings_preserved:true,video_paused_and_callbacks_stopped:true,resume_does_not_autoplay:true,no_page_requests_while_docked:true,resize_alone_does_not_dock:true,mouse_moved:false,real_accounts_used:false};
}catch(error){
  const state=page?await page.evaluate(()=>({width:innerWidth,height:innerHeight,data:{...document.querySelector('[data-testid=shell]').dataset},notice:document.querySelector('.notice')?.textContent})).catch(()=>null):null;
  await writeFile(path.join(output,'dock-failure.json'),JSON.stringify({error:String(error),native:native(),state},null,2));
  throw error;
}finally{
  if(page)await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('window_action',{action:'quit'})).catch(()=>{});
  if(browser)await browser.close().catch(()=>{});
  if(child.exitCode===null){await Promise.race([new Promise(resolve=>child.once('exit',resolve)),delay(10000)]);if(child.exitCode===null){child.kill();throw Error('prototype did not exit normally');}}
}
await writeFile(path.join(output,'dock.json'),JSON.stringify(report,null,2));await rm(path.join(output,'dock-failure.json'),{force:true});console.log(JSON.stringify(report));
