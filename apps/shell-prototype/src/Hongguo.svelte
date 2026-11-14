<script lang="ts">
  import {onDestroy, tick, untrack, type Snippet} from 'svelte';
  import {invoke} from '@tauri-apps/api/core';
  import Icon from './Icon.svelte';
  import {formatTime} from './lifecycle';
  import {loginPending,adjacent,message,qualityLabels,type Account,type Item,type Playback} from './hongguo';
  let {active,showing,accountOnly,notice,navigation}: {active:boolean;showing:boolean;accountOnly:boolean;notice:(text:string)=>void;navigation:Snippet}=$props();
  let account=$state<Account>({state:'idle',name:''});
  let tab=$state<'search'|'favorites'>('search');
  let mode=$state<'list'|'player'>('list');
  let query=$state('');let searched=$state(false);let page=1;
  let items=$state<Item[]>([]);let favorites=$state<Item[]>([]);let hasMore=$state(false);let busy=$state(false);
  let selected=$state<Item>();let episode=$state(1);let episodes=$state<number[]>([]);let quality=$state('720p');let qualities=$state<string[]>([]);
  let source=$state<string>();let lease:number|undefined;let video=$state<HTMLVideoElement>(undefined!);
  let playing=$state(false);let position=$state(0);let duration=$state(0);let muted=$state(true);let volume=$state(.5);let speed=$state(1);
  let playerLoading=$state(false);let seeking=$state<number|null>(null);
  let watched=0;let lastCheckpoint=0;let epoch=0;let playRequest=0;let live=false;let timer:ReturnType<typeof setTimeout>|undefined;let frame:number|undefined;
  let cancelMetadata:(()=>void)|undefined;let history:Record<string,{episode:number;time:number}>={};
  let opening:Promise<Playback>|undefined;
  let direct=$state(localStorage.getItem('lazybuddy.hongguo.direct')!=='false');
  let frameTarget:number|null=null;
  const raw=<T,>(method:string,args:Record<string,unknown>={})=>invoke<T>('playback_call',{method,args,direct});
  async function changeConnection(value:boolean){
    direct=value;localStorage.setItem('lazybuddy.hongguo.direct',String(value));
    notice(value?'红果直连已开启':'红果已恢复跟随系统代理');
    await suspend(false);await activate();
  }
  const valid=(ticket:number)=>live&&active&&ticket===epoch;
  function warn(error:unknown){const value=String(error);if(!/stale_|inactive_|页面已暂停/.test(value))notice(message(error));}
  function stopFrames(){if(video&&frame!==undefined)video.cancelVideoFrameCallback(frame);frame=undefined;}
  function frames(){
    stopFrames();const ticket=epoch;
    const next=(_:number,metadata:VideoFrameCallbackMetadata)=>{
      frame=undefined;if(!valid(ticket)||!video)return;
      if(frameTarget!==null&&Math.abs(metadata.mediaTime-frameTarget)>.2){frame=video.requestVideoFrameCallback(next);return;}
      frameTarget=null;
      watched=metadata.mediaTime;
      if(video.currentTime>=0)position=video.currentTime;
      if(playing&&performance.now()-lastCheckpoint>15000){lastCheckpoint=performance.now();void checkpoint(false);}
      if(!video.paused)frame=video.requestVideoFrameCallback(next);
    };
    frame=video.requestVideoFrameCallback(next);
  }
  async function checkpoint(final:boolean){
    if(lease===undefined||!duration||watched<=0)return;
    const data=await raw<Playback>('playback.checkpoint',{session:lease,position:watched,duration,final});
    if(data.progress_error)notice('观看进度保存失败');
  }
  function detach(){cancelMetadata?.();stopFrames();frameTarget=null;playing=false;if(video){video.pause();video.removeAttribute('src');video.load();}source=undefined;}
  export async function suspend(retain=false){
    const wasLive=live;live=false;epoch++;playRequest++;clearTimeout(timer);timer=undefined;
    if(video&&video.readyState>0)position=video.currentTime;
    if(video)video.pause();stopFrames();playing=false;
    if(wasLive&&source){try{await checkpoint(true);}catch(error){warn(error);}}
    detach();playerLoading=false;busy=false;seeking=null;
    if(!retain)lease=undefined;
    if(loginPending(account.state))account={state:'idle',name:''};
    else account={...account,qr_png:''};
  }
  async function attach(data:Playback,ticket:number){
    if(!valid(ticket)||!data.resource)return;
    lease=data.session;episodes=data.episodes??episodes;qualities=qualityLabels(data.available_qualities??[]);
    const element=video;
    const ready=new Promise<void>((resolve,reject)=>{
      const clear=()=>{element.removeEventListener('loadedmetadata',loaded);element.removeEventListener('error',failed);cancelMetadata=undefined;};
      const loaded=()=>{clear();resolve();};const failed=()=>{clear();reject(new Error('浏览器无法读取或解码该媒体资源'));};
      cancelMetadata=()=>{clear();reject(new Error('页面已暂停'));};
      element.addEventListener('loadedmetadata',loaded,{once:true});element.addEventListener('error',failed,{once:true});
    });
    source=data.resource.url;await tick();await ready;
    if(!valid(ticket))return;
    duration=element.duration;element.playbackRate=speed;position=Math.min(position,Math.max(0,duration-.05));frameTarget=position;frames();element.currentTime=position;
    const actual=qualities.find(value=>Number.parseInt(value)===Math.min(element.videoWidth,element.videoHeight));
    if(actual)quality=actual;
  }
  async function activate(){
    live=true;busy=true;const ticket=++epoch;
    try{
      if(mode==='player'&&lease!==undefined&&!accountOnly){await attach(await raw<Playback>('playback.resume',{session:lease}),ticket);return;}
      lease=undefined;
      const data=await raw<Account>('account.restore');if(!valid(ticket))return;
      account=data;
      if(data.state==='signed_in')await signedIn(ticket);else if(data.error)notice(message(data.error));else notice('红果账号未登录');
    }catch(error){if(valid(ticket)){lease=undefined;warn(error);}}finally{if(valid(ticket))busy=false;}
  }
  $effect(()=>{const enabled=active;const retain=showing&&!accountOnly;untrack(()=>{if(enabled){void activate();}else{void suspend(retain);}});});
  onDestroy(()=>{void suspend(false);});
  function acceptAccount(data:Account){
    account=data;
    if(data.favorites_error)notice(message(data.favorites_error));
    else if(data.operation!=='pending'&&data.favorites)favorites=data.favorites;
  }
  async function signedIn(ticket:number){
    notice(`已登录 ${account.name}`);favorites=[];
    if(accountOnly)return;
    const data=await raw<{history:typeof history}>('progress.read');if(!valid(ticket))return;
    history=data.history??{};
    if(tab==='favorites'&&!accountOnly)await readFavorites();
  }
  function poll(ticket:number){
    clearTimeout(timer);
    timer=setTimeout(async()=>{
      if(!valid(ticket))return;
      try{
        const previous=account.state;const data=await raw<Account>('account.status');if(!valid(ticket))return;acceptAccount(data);
        if(previous!=='signed_in'&&data.state==='signed_in')await signedIn(ticket);
        if(loginPending(data.state)||data.operation==='pending')poll(ticket);
        else if(data.error)notice(message(data.error));
      }catch(error){if(valid(ticket))warn(error);}
    },1000);
  }
  async function login(){
    if(busy)return;busy=true;const ticket=epoch;favorites=[];items=[];history={};
    try{const data=await raw<Account>('account.login');if(!valid(ticket))return;account=data;notice('请使用红果短剧 App 扫码；登录信息安全保存在本机预览环境');poll(ticket);}
    catch(error){if(valid(ticket))warn(error);}finally{if(valid(ticket))busy=false;}
  }
  async function cancelLogin(){
    clearTimeout(timer);const ticket=epoch;
    try{const data=await raw<Account>('account.cancel');if(valid(ticket))account=data;}catch(error){if(valid(ticket))warn(error);}
  }
  async function logout(){
    clearTimeout(timer);detach();lease=undefined;mode='list';favorites=[];items=[];history={};
    const ticket=epoch;
    try{const data=await raw<Account>('account.logout');if(valid(ticket)){account=data;notice('已退出预览账号并删除其保存凭据');}}catch(error){if(valid(ticket))warn(error);}
  }
  async function search(more=false){
    if(busy||!query.trim())return;busy=true;const ticket=epoch;const word=query.trim();const requested=more?page+1:1;
    try{const data=await raw<{items:Item[];has_more:boolean}>('search',{query:word,page:requested});if(!valid(ticket))return;
      items=more?[...items,...data.items]:data.items;page=requested;hasMore=data.has_more;searched=true;notice(`搜索完成：${word}`);
    }catch(error){if(valid(ticket))warn(error);}finally{if(valid(ticket))busy=false;}
  }
  async function readFavorites(){
    const ticket=epoch;
    try{const data=await raw<Account>('favorites.read');if(!valid(ticket))return;acceptAccount(data);if(data.operation==='pending')poll(ticket);}
    catch(error){if(valid(ticket))warn(error);}
  }
  async function chooseTab(value:'search'|'favorites'){tab=value;if(value==='favorites')await readFavorites();}
  async function favorite(item:Item){
    if(account.operation==='pending')return;const ticket=epoch;
    try{const data=await raw<Account>('favorites.set',{series:item.id,favorite:!favorites.some(row=>row.id===item.id)});if(!valid(ticket))return;acceptAccount(data);if(data.operation==='pending')poll(ticket);}
    catch(error){if(valid(ticket))warn(error);}
  }
  async function open(item:Item){if(selected?.id!==item.id)episodes=[];selected=item;episode=history[item.id]?.episode??1;position=history[item.id]?.time??0;watched=position;duration=0;mode='player';await tick();await load(true);}
  async function load(autoplay:boolean){
    if(!selected||!live)return;
    const request=++playRequest;playerLoading=true;const ticket=epoch;const current=()=>valid(ticket)&&request===playRequest;detach();lease=undefined;qualities=[];
    try{
      const pending=raw<Playback>('playback.open',{series:selected.id,episode,quality,origin:location.origin});opening=pending;
      let data=await pending;if(opening===pending)opening=undefined;if(!current())return;lease=data.session;
      const deadline=performance.now()+60000;
      while(data.loading&&current()&&performance.now()<deadline){await new Promise(resolve=>setTimeout(resolve,200));if(!current())return;data=await raw<Playback>('playback.status');}
      if(!current())return;
      if(data.error)throw new Error(data.error);
      if(!data.prepared){await raw('playback.close',{session:lease});lease=undefined;throw new Error('媒体资源准备超时');}
      await attach(data,ticket);if(!current())return;
      if(autoplay){await video.play();if(!current())return;}
      notice(`${selected.title} 第${episode}集`);
    }catch(error){if(current())warn(error);}finally{if(current())playerLoading=false;}
  }
  export async function toggle(){
    if(!live||mode!=='player')return;
    if(!source||video.error){await load(true);return;}
    if(video.paused){try{await video.play();}catch(error){warn(error);}}
    else{video.pause();void checkpoint(true).catch(warn);}
  }
  export function pause(){if(video)video.pause();stopFrames();playing=false;}
  export function seekRelative(delta:number){if(video&&source){frameTarget=Math.max(0,Math.min(duration,video.currentTime+delta));frames();video.currentTime=frameTarget;}}
  async function videoError(){
    if(!live||!source||!video?.error)return;
    playing=false;stopFrames();
    const ticket=epoch;const request=playRequest;const code=video.error.code;
    try{const data=await raw<Playback>('playback.status');if(!valid(ticket)||request!==playRequest)return;
      if(data.error)warn(data.error);else notice(code===4?'浏览器不支持该视频编码或封装':code===3?'视频解码失败':'媒体读取失败');
    }catch(error){if(valid(ticket))warn(error);}
  }
  async function leavePlayer(){playRequest++;pause();const pending=opening;await checkpoint(true).catch(warn);detach();const old=lease;lease=undefined;mode='list';
    try{const session=old??(pending? (await pending).session:undefined);if(session!==undefined)await raw('playback.close',{session});}catch(error){if(String(error)!=='media_not_loaded')warn(error);}}
  async function changeEpisode(value:number){if(value===episode)return;await checkpoint(true).catch(warn);episode=value;position=watched=0;await load(true);}
  async function changeQuality(value:string){quality=value;position=video?.currentTime??position;await load(playing);}
  function ended(){if(!live||!active||playerLoading||!video.ended)return;playing=false;stopFrames();void checkpoint(true).catch(warn);const next=adjacent(episodes,episode,1);if(next!==undefined)void changeEpisode(next);}
</script>

{#if showing}
  {#if accountOnly}
    <section class="settings-layout content-width">{@render navigation()}<div class="settings-content account-content"><h1>红果账号</h1><label><input type="checkbox" checked={direct} disabled={busy||loginPending(account.state)||account.operation==='pending'} onchange={event=>changeConnection(event.currentTarget.checked)}/>直连</label>{#if account.state==='signed_in'}<p>{account.name}</p><button class="logout" onclick={logout}>退出登录</button>{:else}<button onclick={login} disabled={busy||loginPending(account.state)}>登录</button>{/if}</div></section>
  {:else if account.state!=='signed_in'}
    <section class="account-login"><button onclick={login} disabled={busy||loginPending(account.state)}>登录</button></section>
  {:else if mode==='player'}
    <section class="player-page"><div class="video-stage" class:paused={!playing}>
      <video bind:this={video} src={source} preload="metadata" crossorigin="anonymous" playsinline bind:muted bind:volume ontimeupdate={()=>{if(video.readyState>0&&seeking===null)position=video.currentTime;}} onplay={()=>{if(!live){video.pause();return;}playing=true;frames();}} onpause={()=>{playing=false;stopFrames();}} onended={ended} onerror={videoError} aria-label="红果短剧视频"><track kind="captions"/></video>
      <div class="video-top"><button class="video-back" aria-label="返回列表" onclick={leavePlayer}><Icon name="arrow" size={17}/></button><span class="video-title">{selected?.title} 第{episode}集</span><button class="video-favorite" aria-label={favorites.some(row=>row.id===selected?.id)?'取消收藏':'收藏'} onclick={()=>selected&&favorite(selected)} disabled={account.operation==='pending'}><Icon name="star"/></button></div>
      {#if !playing}<button class="center-play" aria-label={playerLoading?'加载中':'播放视频'} disabled={playerLoading} onclick={toggle}><Icon name="play" size={28}/></button>{/if}
      <div class="video-bottom"><input class="seek" aria-label="播放进度" type="range" min="0" max={duration||1} step=".05" value={seeking??position} disabled={!source} oninput={event=>seeking=Number(event.currentTarget.value)} onchange={()=>{if(seeking!==null){frameTarget=seeking;frames();video.currentTime=seeking;}seeking=null;}}/><div class="video-controls">
        <button aria-label="上一集" disabled={adjacent(episodes,episode,-1)===undefined} onclick={()=>{const value=adjacent(episodes,episode,-1);if(value!==undefined)void changeEpisode(value);}}><Icon name="previous"/></button>
        <button aria-label={playing?'暂停':'播放'} disabled={playerLoading} onclick={toggle}><Icon name={playing?'pause':'play'}/></button>
        <button aria-label="下一集" disabled={adjacent(episodes,episode,1)===undefined} onclick={()=>{const value=adjacent(episodes,episode,1);if(value!==undefined)void changeEpisode(value);}}><Icon name="next"/></button>
        <span class="time">{formatTime(seeking??position)} / {formatTime(duration)}</span><span class="control-spacer"></span>
        {#if episodes.length}<select aria-label="选集" value={episode} onchange={event=>changeEpisode(Number(event.currentTarget.value))}>{#each episodes as value}<option value={value}>第{value}集</option>{/each}</select>{/if}
        {#if qualities.length}<select aria-label="清晰度" value={quality} onchange={event=>changeQuality(event.currentTarget.value)}>{#each qualities as value}<option value={value}>{value}</option>{/each}</select>{/if}
        <select aria-label="倍速" value={speed} onchange={event=>{speed=Number(event.currentTarget.value);video.playbackRate=speed;}}>{#each [.5,1,1.5,2] as value}<option value={value}>{value}×</option>{/each}</select>
        <button aria-label={muted?'取消静音':'静音'} onclick={()=>muted=!muted}><Icon name={muted?'mute':'volume'}/></button><input class="volume" aria-label="音量" type="range" min="0" max="1" step=".05" bind:value={volume}/>
      </div></div>
    </div></section>
  {:else}
    <section class="media-library"><div class="list-toolbar"><div class="tabs"><button class:active={tab==='search'} onclick={()=>chooseTab('search')}>搜索</button><button class:active={tab==='favorites'} onclick={()=>chooseTab('favorites')}>收藏</button></div></div>
      {#if tab==='search'}<form class="media-search" role="search" onsubmit={event=>{event.preventDefault();void search();}}><label class="search-box"><Icon name="search" size={16}/><input aria-label="搜索短剧" placeholder="搜索短剧" bind:value={query}/></label><button type="submit" disabled={busy}>搜索</button></form>{/if}
      {#each tab==='search'?items:favorites as item (item.id)}<div class="media-result"><div><h2>{item.title}</h2>{#if item.episodes}<span>共 {item.episodes} 集</span>{/if}</div><div class="row-actions"><button aria-label={favorites.some(row=>row.id===item.id)?`取消收藏${item.title}`:`收藏${item.title}`} disabled={account.operation==='pending'} onclick={()=>favorite(item)}><Icon name="star" size={15}/></button><button aria-label={`播放${item.title}`} onclick={()=>open(item)}><Icon name="play" size={15}/>播放</button></div></div>{/each}
      {#if tab==='search'&&hasMore}<button disabled={busy} onclick={()=>search(true)}>更多</button>{/if}
      {#if tab==='favorites'&&favorites.length===0&&account.operation!=='pending'}<div class="list-empty">暂无收藏</div>{:else if tab==='search'&&searched&&items.length===0}<div class="list-empty">没有搜索结果</div>{/if}
    </section>
  {/if}
  {#if loginPending(account.state)}<div class="qr-backdrop"><div class="qr-dialog" role="dialog" aria-modal="true" aria-label="扫码登录" tabindex="-1"><h2>使用红果短剧 App 扫码</h2>{#if account.qr_png}<img src={`data:image/png;base64,${account.qr_png}`} alt="红果登录二维码"/>{/if}<button onclick={cancelLogin}>取消</button></div></div>{/if}
{/if}

<style>
  .account-login{height:100%;display:grid;place-items:center}.account-content{display:flex;flex-direction:column}.logout{align-self:flex-start;margin-top:auto}
  .qr-backdrop{position:fixed;inset:62px 0 30px;display:grid;place-items:center;background:#0005;z-index:10}.qr-dialog{display:flex;flex-direction:column;align-items:center;gap:12px;padding:20px;background:var(--surface);border:1px solid var(--line)}.qr-dialog h2{font-size:15px;margin:0}.qr-dialog img{width:220px;height:220px;object-fit:contain;background:white}.qr-dialog button{align-self:stretch}
  .video-controls select{width:auto;max-width:85px;min-width:44px;color:white;background:#222c;padding:3px;border:0;font-size:11px}.video-controls{flex-wrap:wrap}.video-title{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
</style>
