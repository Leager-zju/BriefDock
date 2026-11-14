<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Icon from './Icon.svelte';
  import Hongguo from './Hongguo.svelte';
  import { accepts, active, formatTime, toggleSection, moduleTarget, type Root, type Section, type View } from './lifecycle';
  import logoUrl from '../../../assets/lazybuddy.svg';
  type Source = { id: string; name: string; tag: string; color: string; items: string[] };
  let root = $state<Root>('news');
  let section = $state<Section>('content');
  let docked = $state(false);
  let visible = $state(!document.hidden);
  let view = $state<View>({ root: 'news', section: 'content', visible: !document.hidden, generation: 0 });
  let dark = $state(false);
  let notice = $state('离线样例 · 不连接真实新闻或红果账号');
  let feeds = $state<Source[]>([]);
  let filter = $state('all');
  let query = $state('');
  let submittedQuery = $state('');
  let mediaMode = $state<'search' | 'favorites' | 'player'>('search');
  let returnTo = $state<'search' | 'favorites'>('search');
  let sampleFavorite = $state(false);
  let loading = $state(false);
  let category = $state('page');
  let video = $state<HTMLVideoElement>(undefined!);
  let servicePlayback = $state(false);
  let serviceActive = $state(false);
  let realHongguo = $state<Hongguo>();
  let lastAccountPage = false;
  let playing = $state(false);
  let position = $state(0);
  let duration = $state(0);
  let muted = $state(true);
  let volume = $state(0.5);
  let frameCount = $state(0);
  let callback: number | undefined;
  let sequence = 0;
  let followed = $state<string[]>([]);
  const scrollPositions = new Map<string, number>();
  let page: HTMLElement;
  let bridge = $state(false);
  let eventsReady = $state(false);
  const native = '__TAURI_INTERNALS__' in window;
  async function call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
    if (!native) throw new Error('请在原型桌面窗口中使用');
    return invoke<T>(name, args);
  }
  function stopFrames() {
    if (video && callback !== undefined) video.cancelVideoFrameCallback(callback);
    callback = undefined;
  }
  function pause() { if (video) video.pause(); stopFrames(); playing = false; }
  function mediaState(event: Event) {
    if (event.currentTarget !== video) return;
    playing = !video.paused;
    if (playing) trackFrames(); else stopFrames();
  }
  function trackFrames() {
    stopFrames();
    const frame = () => {
      callback = undefined;
      if (!active(view, 'hongguo') || video.paused) return;
      frameCount += 1;
      callback = video.requestVideoFrameCallback(frame);
    };
    callback = video.requestVideoFrameCallback(frame);
  }
  async function change(nextRoot = root, nextSection = section, nextVisible = visible) {
    const accountPage=nextRoot==='hongguo'&&nextSection==='settings'&&category==='page';
    if (view.generation > 0 && nextRoot === root && nextSection === section && nextVisible === visible && accountPage===lastAccountPage) return;
    const ticket = ++sequence;
    serviceActive=false;
    await realHongguo?.suspend(nextRoot==='hongguo'&&nextSection==='content');
    if(ticket!==sequence)return;
    lastAccountPage=accountPage;
    if (page && visible && !docked) scrollPositions.set(`${root}:${section}`, page.scrollTop);
    pause();
    root = nextRoot; section = nextSection; visible = nextVisible;
    view = { root, section, visible, generation: view.generation + 1 };
    loading = false;
    await tick();
    if (ticket !== sequence) return;
    if (page) page.scrollTop = scrollPositions.get(`${root}:${section}`) ?? 0;
    try {
      const lease = await call<View>('transition', { root, section, visible, accountPage });
      if (ticket !== sequence) return;
      view = lease;
      serviceActive = root==='hongguo' && visible && (section==='content'||accountPage);
      if (active(view, 'news') && feeds.length === 0) await refresh();
    } catch (error) { if(ticket===sequence) notice = String(error); }
  }
  async function refresh() {
    if (!active(view, 'news') || loading) return;
    const request = { ...view };
    loading = true;
    try {
      const reply = await call<Source[]>('demo_feed', { generation: request.generation });
      if (accepts(view, request)) { feeds = reply; notice = '样例列表已更新 · 未请求外部服务'; }
    } catch (error) { if (accepts(view, request)) notice = `无法获取样例：${error}`; }
    finally { if (accepts(view, request)) loading = false; }
  }
  async function action(action: string) {
    if (action !== 'show') { pause(); await realHongguo?.suspend(action!=='quit'); }
    try { await call('window_action', { action }); } catch (error) { notice = String(error); }
  }
  function toggleModules() {
    void change(root, moduleTarget(section));
  }
  async function togglePlayback() {
    if (!active(view, 'hongguo') || mediaMode !== 'player' || !video) return;
    if (!video.paused) { pause(); return; }
    const element = video;
    const generation = view.generation;
    try {
      await element.play();
      if (element !== video || !active(view, 'hongguo') || mediaMode !== 'player' || generation !== view.generation) { element.pause(); return; }
      playing = true; trackFrames(); notice = '正在播放本地样例';
    }
    catch { notice = '本地样例未加载，请通过原型启动脚本准备视频。'; }
  }
  async function openPlayer() {
    returnTo = mediaMode === 'favorites' ? 'favorites' : 'search';
    mediaMode = 'player';
    await tick();
    await togglePlayback();
  }
  function returnToList() { pause(); mediaMode = returnTo; }
  function favoriteSample() {
    sampleFavorite = !sampleFavorite;
    notice = sampleFavorite ? '已收藏本地样例 · 未写入红果账号' : '已取消本地样例收藏 · 未写入红果账号';
  }
  function mediaKey(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement || event.target instanceof HTMLButtonElement) return;
    if (!active(view, 'hongguo')) return;
    if(servicePlayback){if(event.code==='Space'){event.preventDefault();void realHongguo?.toggle();}else if(['ArrowRight','ArrowLeft'].includes(event.code)){event.preventDefault();realHongguo?.seekRelative(event.code==='ArrowRight'?5:-5);}return;}
    if(mediaMode!=='player')return;
    if (event.code === 'Space') { event.preventDefault(); void togglePlayback(); }
    if (event.code === 'ArrowRight' || event.code === 'ArrowLeft') { event.preventDefault(); video.currentTime = Math.max(0, Math.min(duration, video.currentTime + (event.code === 'ArrowRight' ? 5 : -5))); }
  }
  onMount(() => {
    const unlisteners: (() => void)[] = [];
    let disposed = false;
    function subscribe<T>(event: string, handler: (value: T) => void) {
      return listen<T>(event, value => handler(value.payload)).then(dispose => {
        if (disposed) dispose(); else unlisteners.push(dispose);
      });
    }
    // Native window events own page activation; late browser visibility events only pause media.
    const visibility = () => { if (document.hidden) { pause();realHongguo?.pause(); } };
    document.addEventListener('visibilitychange', visibility);
    if (native) {
      void Promise.all([
      subscribe<boolean>('shell-visibility', value => { void change(root, section, value); }),
      subscribe<boolean>('shell-docked', value => {
        docked = value;
        if (value) pause();
        else void tick().then(() => { if (page) page.scrollTop = scrollPositions.get(`${root}:${section}`) ?? 0; });
      }),
      subscribe<string>('shell-error', value => { notice = value; })
      ]).then(() => { eventsReady = true; }).catch(() => { notice = '窗口通知订阅失败，请重新打开原型。'; });
      void call('contract_templates').then(() => { bridge = true; });
      void call<{media_route:string}>('snapshot').then(value=>{servicePlayback=value.media_route==='browser_service';if(servicePlayback)notice='红果真实服务已就绪，选择红果短剧后登录';});
      void change();
    } else notice = '浏览器设计预览 · 原生交互请启动独立桌面原型';
    return () => { disposed = true; pause(); unlisteners.forEach(dispose => dispose()); document.removeEventListener('visibilitychange', visibility); };
  });
</script>

<svelte:window onkeydown={mediaKey}/>
{#snippet settingsNavigation()}<aside class="settings-nav" aria-label="设置分类"><div class="eyebrow">当前功能</div><button class:chosen={category === 'page'} onclick={() => { category = 'page';void change(); }}>{root === 'news' ? '新闻设置' : '红果账号'}</button><div class="eyebrow">通用</div>{#each [['appearance', '显示设置'], ['system', '系统设置'], ['about', '关于我们']] as [id, label]}<button class:chosen={category === id} onclick={() => { category = id;void change(); }}>{label}</button>{/each}<button class="back-button" onclick={() => change(root, 'content')}><Icon name="arrow" size={16}/>返回页面</button></aside>{/snippet}
<div class:dark class:docked class="app-shell" data-testid="shell" data-root={root} data-section={section} data-visible={visible} data-docked={docked} data-events-ready={eventsReady} data-frames={frameCount}>
  {#if docked}<button class="dock-button" aria-label="展开 LazyBuddy" title="展开 LazyBuddy" onclick={() => action('expand')}><img src={logoUrl} alt=""/></button>{/if}
  <header class="toolbar" data-tauri-drag-region>
    <div class="identity" data-tauri-drag-region><span class="logo" data-tauri-drag-region>LB<span>·</span></span><strong data-tauri-drag-region>LazyBuddy</strong><span class="divider"></span>
      <label class="root-selector"><span class="status-dot"></span><select aria-label="选择功能" value={root} onchange={event => change(event.currentTarget.value as Root)}><option value="news">新闻速览</option><option value="hongguo">红果短剧</option></select><Icon name="chevron" size={14}/></label>
    </div>
    <div class="toolbar-spacer" data-tauri-drag-region></div>
    <nav aria-label="全局操作">
      <button class="icon-button" class:chosen={section === 'modules'} onclick={toggleModules} aria-label="功能" title="功能" aria-pressed={section === 'modules'}><Icon name="box"/></button>
      <button class="icon-button" class:chosen={section === 'settings'} onclick={() => change(root, toggleSection(section, 'settings'))} aria-label="设置" title="设置" aria-pressed={section === 'settings'}><Icon name="settings"/></button>
      <span class="divider"></span>
      <button class="icon-button close-button" aria-label="关闭到托盘" title="关闭到托盘" onclick={() => action('close')}><Icon name="close"/></button>
    </nav>
  </header>

  <main class="page" bind:this={page}>
    {#if servicePlayback}<Hongguo bind:this={realHongguo} active={serviceActive&&!docked} showing={root==='hongguo'&&(section==='content'||section==='settings'&&category==='page')} accountOnly={section==='settings'} notice={text=>notice=text} navigation={settingsNavigation}/>{/if}
    {#if section === 'modules'}
      <section class="content-width module-page">
        <div class="module-heading"><h1>可用功能</h1><button onclick={toggleModules}><Icon name="arrow" size={16}/>返回</button></div>
        <div class="module-list">
          {#each [{ name: '新闻速览', version: '0.1.0' }, { name: '红果短剧', version: '0.1.0' }] as module}
            <div class="module-row"><div><strong>{module.name}</strong><span class="version">{module.version}</span></div><div class="row-actions"><button disabled>更新</button><button class="subtle-danger" onclick={() => notice = '这是界面原型，未改动任何已安装功能。'}>卸载</button></div></div>
          {/each}
        </div>
      </section>
    {:else if section === 'settings'}
      {#if !(servicePlayback&&root==='hongguo'&&category==='page')}
      <section class="settings-layout content-width">
        {@render settingsNavigation()}
        <div class="settings-content">
          <h1>{category === 'page' ? (root === 'news' ? '新闻设置' : '短剧设置') : category === 'appearance' ? '显示设置' : category === 'system' ? '系统设置' : '关于我们'}</h1>
          {#if category === 'appearance'}
            <div class="setting-row"><div><strong>外观</strong><p>选择适合当前环境的界面主题</p></div><button onclick={() => dark = !dark}><Icon name={dark ? 'sun' : 'moon'}/>{dark ? '切换浅色' : '切换深色'}</button></div>
            <div class="setting-row"><div><strong>界面动效</strong><p>仅在操作时短暂过渡，静止时不持续绘制</p></div><span class="tag">按需</span></div>
          {:else if category === 'system'}
            <div class="setting-row"><div><strong>应用自动更新</strong><p>正式宿主仍使用已有签名更新器</p></div><span class="tag">原型不执行更新</span></div><div class="setting-row"><div><strong>后台行为</strong><p>收起后暂停视频和页面任务</p></div><span class="tag">事件驱动</span></div>
          {:else if category === 'about'}
            <div class="about-mark">LB<span>·</span></div><h2>LazyBuddy</h2><p class="muted">独立界面原型 · Tauri 2 / Svelte 5</p><p class="muted">Copyright © 2026 Leager · MIT</p>
          {:else if root === 'news'}
            <div class="setting-row"><div><strong>自动刷新</strong><p>原型使用离线内容，不启用后台刷新</p></div><span class="tag">已暂停</span></div>
            <h2 class="section-title">来源管理</h2>
            {#each [...feeds, ...feeds, ...feeds] as source, index}<div class="source-setting"><span>{source.name}<small> / {index + 1}</small></span><div class="row-actions"><button onclick={() => notice = '关注操作仅作样式演示。'}>关注</button><button onclick={() => notice = '隐藏操作仅作样式演示。'}>隐藏</button><button onclick={() => notice = '置顶操作仅作样式演示。'}>置顶</button></div></div>{/each}
          {:else}
            <div class="setting-row"><div><strong>播放与账号</strong><p>本原型只播放本地合成视频，不访问真实账号</p></div><span class="tag">样例模式</span></div>
            <div class="setting-row"><div><strong>离开播放页</strong><p>停止视频推进，返回后由用户主动播放</p></div><span class="tag">已启用</span></div>
          {/if}
        </div>
      </section>
      {/if}
    {:else if root === 'news'}
      <section class="content-width news-page">
        <div class="list-toolbar"><div class="tabs"><button class:active={filter === 'all'} onclick={() => filter = 'all'}>全部来源<span>3</span></button><button class:active={filter === 'followed'} onclick={() => filter = 'followed'}>已关注<span>{followed.length}</span></button></div><button class="refresh-button" onclick={refresh} disabled={loading}><Icon name="refresh" size={15}/>{loading ? '获取中' : '刷新'}</button></div>
        <div class="source-grid">
          {#each feeds.filter(source => filter === 'all' || followed.includes(source.id)) as source}
            <article class="source-card"><div class="source-heading"><span class="source-mark {source.color}">{source.tag}</span><div><h2>{source.name}</h2><span>热榜</span></div><button class="icon-button favorite" class:followed={followed.includes(source.id)} aria-label={`关注${source.name}`} onclick={() => followed = followed.includes(source.id) ? followed.filter(id => id !== source.id) : [...followed, source.id]}><Icon name="star" size={16}/></button></div><ol>{#each source.items as item, index}<li><span class:top-rank={index < 3} class="rank">{String(index + 1).padStart(2, '0')}</span><button title={item} onclick={() => notice = '离线样例未连接外部新闻链接。'}>{item}</button></li>{/each}</ol><div class="card-foot"><span>{source.items.length} 条样例</span><span>刚刚更新</span></div></article>
          {/each}
        </div>
        {#if filter === 'followed' && followed.length === 0}<div class="list-empty">暂无关注来源</div>{/if}
      </section>
    {:else if !servicePlayback}
      {#if mediaMode === 'player'}
        <section class="player-page">
          <div class="video-stage" class:paused={!playing}>
            <video bind:this={video} src="http://sample.localhost/clip" preload="metadata" crossorigin="anonymous" playsinline bind:muted bind:volume ontimeupdate={() => { if(video.readyState>0) position = video.currentTime; }} onloadedmetadata={() => { duration = video.duration; video.currentTime = Math.min(position, duration); }} onplay={mediaState} onpause={mediaState} onended={mediaState} onerror={() => notice = '本地视频未准备，请通过原型启动脚本生成样例。'} aria-label="本地合成测试视频"><track kind="captions"/></video>
            <div class="video-top"><button class="video-back" aria-label="返回列表" onclick={returnToList}><Icon name="arrow" size={17}/></button><span class="video-title">本地播放样例</span><button class="video-favorite" aria-label={sampleFavorite ? '取消收藏样例' : '收藏样例'} onclick={favoriteSample}><Icon name="star" size={18}/></button></div>
            {#if !playing}<button class="center-play" aria-label="播放样例" onclick={togglePlayback}><Icon name="play" size={28}/></button>{/if}
            <div class="video-bottom"><input class="seek" aria-label="播放进度" type="range" min="0" max={duration || 1} step="0.05" value={position} oninput={event => position = Number(event.currentTarget.value)} onchange={() => video.currentTime = position}/><div class="video-controls"><button aria-label={playing ? '暂停' : '播放'} onclick={togglePlayback}><Icon name={playing ? 'pause' : 'play'}/></button><span class="time">{formatTime(position)}<span> / {formatTime(duration)}</span></span><span class="control-spacer"></span><button aria-label={muted ? '取消静音' : '静音'} onclick={() => muted = !muted}><Icon name={muted ? 'mute' : 'volume'}/></button><input class="volume" aria-label="音量" type="range" min="0" max="1" step="0.05" bind:value={volume}/></div></div>
          </div>
        </section>
      {:else}
        <section class="media-library">
          <div class="list-toolbar"><div class="tabs"><button class:active={mediaMode === 'search'} onclick={() => mediaMode = 'search'}>搜索</button><button class:active={mediaMode === 'favorites'} onclick={() => mediaMode = 'favorites'}>收藏</button></div></div>
          {#if mediaMode === 'search'}
            <form class="media-search" role="search" onsubmit={event => { event.preventDefault(); submittedQuery = query.trim(); notice = '离线样例搜索完成 · 未连接红果账号'; }}><label class="search-box"><Icon name="search" size={16}/><input aria-label="搜索短剧" placeholder="搜索短剧" bind:value={query}/></label><button type="submit">搜索</button></form>
          {/if}
          {#if mediaMode === 'favorites' ? sampleFavorite : '本地播放样例'.includes(submittedQuery)}
            <div class="media-result"><div><h2>本地播放样例</h2><span>共 1 集</span></div><div class="row-actions"><button aria-label={sampleFavorite ? '取消收藏样例' : '收藏样例'} onclick={favoriteSample}><Icon name="star" size={15}/>{sampleFavorite ? '已收藏' : '收藏'}</button><button aria-label="打开播放页" onclick={openPlayer}><Icon name="play" size={15}/>播放</button></div></div>
          {:else}<div class="list-empty">{mediaMode === 'favorites' ? '暂无收藏' : '没有搜索结果'}</div>{/if}
        </section>
      {/if}
    {/if}
  </main>
  <footer class="info-bar" aria-live="polite"><span class="status-dot"></span><span class="notice">{notice}</span><span class="footer-detail">{bridge ? '就绪' : '连接中'}<span>·</span>界面原型</span></footer>
</div>
