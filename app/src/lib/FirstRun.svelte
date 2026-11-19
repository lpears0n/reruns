<!-- The empty library needs one instruction, with setup only when needed. -->
<script lang="ts">
  import type { Status } from './api'
  import type { Category } from './SettingsView.svelte'

  let {
    hotkey,
    status,
    onchoosescreen,
    onsettings,
  }: {
    hotkey: string
    status: Status | null
    onchoosescreen: () => void
    onsettings: (c: Category) => void
  } = $props()

  const setup = $derived(status?.needs_screen ? 'screen' : status?.state === 'unavailable' || status?.state === 'retrying' ? 'recording' : null)
</script>

<section class="first-run" aria-label="Getting started">
  <h2>No clips yet</h2>
  <p>Press <span class="hotkey">{hotkey}</span> while playing to save a clip.</p>

  {#if setup}
    <div class="setup-action">
      {#if setup === 'screen'}
        <button class="btn-primary" onclick={onchoosescreen}>Choose screen</button>
      {:else}
        <button class="btn-sm" onclick={() => onsettings('Recording')}>Recording settings</button>
      {/if}
    </div>
  {/if}

  <div class="setup-links">
    <button class="btn-text" onclick={() => onsettings('Hotkeys')}>Change hotkey</button>
    <button class="btn-text" onclick={() => onsettings('Storage')}>Clips folder</button>
  </div>
</section>

<style>
  .first-run {
    display: flex;
    flex-direction: column;
    justify-content: center;
    width: 100%;
    max-width: 420px;
    min-height: 240px;
    height: 100%;
    margin-inline: auto;
    padding-block: 24px 48px;
  }

  h2 {
    font-size: 20px;
    line-height: 28px;
    font-weight: 600;
    letter-spacing: -0.25px;
  }

  p {
    margin-top: 7px;
    color: var(--color-fg-2);
    line-height: 22px;
    overflow-wrap: anywhere;
  }

  .hotkey { color: var(--color-fg); font-weight: 600; }
  .setup-action { margin-top: 18px; }

  .setup-links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 16px;
    margin-left: -8px;
  }

  .setup-links :global(button) { color: var(--color-muted); font-size: var(--text-secondary); }
  .setup-links :global(button:hover) { color: var(--color-fg); }
</style>
