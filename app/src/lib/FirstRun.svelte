<!-- The library before its first clip: what is set up, and where to change it. -->
<script lang="ts">
  import type { Settings, Status } from './api'
  import type { Category } from './SettingsView.svelte'

  let {
    hotkey,
    status,
    settings,
    onchoosescreen,
    onsettings,
  }: {
    hotkey: string
    status: Status | null
    settings: Settings | null
    onchoosescreen: () => void
    onsettings: (c: Category) => void
  } = $props()

  const capture = $derived.by(() => {
    if (!status) return { ok: false, text: 'Starting…' }
    if (status.needs_screen) return { ok: false, text: 'Choose the screen to record. Your desktop asks once.', action: 'screen' as const }
    if (status.state === 'unavailable') return { ok: false, text: status.error ?? 'Capture unavailable.', action: 'recording' as const }
    return {
      ok: true,
      text:
        status.backend === 'kwin' || status.backend === 'windows'
          ? 'Starts by itself when a game opens. No prompts.'
          : 'Records the screen you chose when a game opens.',
    }
  })
  const folder = $derived(settings?.clips_dir.replace(/^\/home\/[^/]+/, '~') ?? '…')
</script>

<div class="mx-auto flex h-full max-w-xl flex-col justify-center pb-10">
  <h2 class="text-title font-semibold">No clips yet</h2>
  <p class="mt-1 text-fg-2">Play something, then press {hotkey} to keep the last {settings?.replay_seconds ?? 30} seconds.</p>

  <ol class="mt-5 divide-y divide-line border-y border-line">
    <li class="flex min-h-16 items-center gap-3 py-3">
      <!-- Only capture has a state to report: a dot, as in the capture strip. -->
      <span class="size-2 flex-none rounded-full {capture.ok ? 'bg-signal-ok' : 'bg-warn'}" data-ok={capture.ok}></span>
      <div class="min-w-0 flex-1">
        <div class="text-fg">Capture</div>
        <div class="truncate text-secondary text-fg-2" title={capture.text}>{capture.text}</div>
      </div>
      {#if capture.action === 'screen'}
        <button class="btn-primary" onclick={onchoosescreen}>Choose screen</button>
      {:else if capture.action === 'recording'}
        <button class="btn-sm" onclick={() => onsettings('Recording')}>Recording settings</button>
      {/if}
    </li>
    <li class="flex min-h-16 items-center gap-3 py-3">
      <span class="size-2 flex-none"></span>
      <div class="min-w-0 flex-1">
        <div class="text-fg">Save replay</div>
        <div class="text-secondary text-fg-2">{hotkey} in any game.</div>
      </div>
      <button class="btn-sm" onclick={() => onsettings('Hotkeys')}>Change</button>
    </li>
    <li class="flex min-h-16 items-center gap-3 py-3">
      <span class="size-2 flex-none"></span>
      <div class="min-w-0 flex-1">
        <div class="text-fg">Clips folder</div>
        <div class="truncate text-secondary text-fg-2" title={settings?.clips_dir}>{folder}</div>
      </div>
      <button class="btn-sm" onclick={() => onsettings('Storage')}>Change</button>
    </li>
  </ol>
  <p class="mt-4 text-secondary text-fg-2">Drop any video here to trim it.</p>
</div>
