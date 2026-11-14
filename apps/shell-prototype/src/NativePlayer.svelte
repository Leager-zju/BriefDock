<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Icon from './Icon.svelte';
  import { formatTime } from './lifecycle';
  let { back, favorite, favorited, changed, failed } : {
    back: () => void; favorite: () => void; favorited: boolean;
    changed: (playback: Status) => void; failed: (error: string) => void;
  } = $props();
  type Status = { session: number; loaded: boolean; playing: boolean; eos: boolean; position: number; duration: number; frames: number; speed: number; volume: number; mute: boolean; error: string | null };
  let playback = $state<Status>({ session: 0, loaded: false, playing: false, eos: false, position: 0, duration: 0, frames: 0, speed: 1, volume: .5, mute: true, error: null });
  let stage: HTMLDivElement;
  let preview = $state<number | null>(null);
  let disposed = false;
  let ready: Promise<unknown>;
  let resolveReady: () => void;
  ready = new Promise<void>(resolve => resolveReady = resolve);
  const area = () => { const { x, y, width, height } = stage.getBoundingClientRect(); return { x, y, width, height }; };
  function accept(value: Status) {
    if (disposed || value.session < playback.session) return;
    playback = value; changed(value);
    if (value.error) failed(value.error);
  }
  async function control(method: string, value?: number | boolean) {
    await ready;
    if (disposed) return;
    try { accept(await invoke<Status>('native_control', { method, value })); }
    catch (error) { if (!disposed) failed(String(error)); }
  }
  export async function toggle() { await ready; if (playback.eos) await control('seek', 0); await control(playback.playing ? 'pause' : 'play'); }
  export function pause() { if (playback.loaded) void control('pause'); }
  export function seekRelative(delta: number) { void control('seek', Math.max(0, Math.min(playback.duration, playback.position + delta))); }
  onMount(() => {
    let unlisten: (() => void) | undefined;
    const observer = new ResizeObserver(() => { const rect = area(); if (!disposed && rect.width > 0 && rect.height > 0) void invoke('native_layout', { area: rect }).catch(error => failed(String(error))); });
    void (async () => {
      try {
        const dispose = await listen<Status>('native-media-state', event => accept(event.payload));
        if (disposed) { dispose(); return; }
        unlisten = dispose;
        accept(await invoke<Status>('native_open', { area: area() }));
        if (!disposed) observer.observe(stage);
      } catch (error) { if (!disposed) failed(String(error)); }
      finally { resolveReady(); }
    })();
    return () => { disposed = true; observer.disconnect(); unlisten?.(); void invoke('native_close'); };
  });
</script>

<div class="video-stage native-stage" class:paused={!playback.playing} bind:this={stage} data-native-video data-frames={playback.frames}>
  <div class="video-top"><button class="video-back" aria-label="返回列表" onclick={back}><Icon name="arrow" size={17}/></button><span class="video-title">本地播放样例</span><button class="video-favorite" aria-label={favorited ? '取消收藏样例' : '收藏样例'} onclick={favorite}><Icon name="star" size={18}/></button></div>
  {#if !playback.playing}<button class="center-play" aria-label="播放样例" onclick={toggle}><Icon name="play" size={28}/></button>{/if}
  <div class="video-bottom"><input class="seek" aria-label="播放进度" type="range" min="0" max={playback.duration || 1} step="0.05" value={preview ?? playback.position} oninput={event => preview = Number(event.currentTarget.value)} onchange={() => { if (preview !== null) void control('seek', preview); preview = null; }}/><div class="video-controls"><button aria-label={playback.playing ? '暂停' : '播放'} onclick={toggle}><Icon name={playback.playing ? 'pause' : 'play'}/></button><span class="time">{formatTime(preview ?? playback.position)}<span> / {formatTime(playback.duration)}</span></span><span class="control-spacer"></span><select aria-label="倍速" value={playback.speed} onchange={event => control('speed', Number(event.currentTarget.value))}>{#each [.5, 1, 1.5, 2] as speed}<option value={speed}>{speed}×</option>{/each}</select><button aria-label={playback.mute ? '取消静音' : '静音'} onclick={() => control('mute', !playback.mute)}><Icon name={playback.mute ? 'mute' : 'volume'}/></button><input class="volume" aria-label="音量" type="range" min="0" max="1" step="0.05" value={playback.volume} onchange={event => control('volume', Number(event.currentTarget.value))}/></div></div>
</div>


