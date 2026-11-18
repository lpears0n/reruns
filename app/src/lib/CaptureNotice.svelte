<!-- App-level feedback. The app owns placement, expiry and source-bound
     callbacks; the notice only presents the operation and its actions. -->
<script lang="ts">
  import { X } from '@lucide/svelte'
  import { fly } from 'svelte/transition'
  import { fileUrl } from './api'
  import { duration, gameName } from './format'
  import type { Notice } from './types'

  let {
    notice,
    ondismiss,
    copyBlocked = false,
    onpause,
    onresume,
  }: {
    notice: Notice | null
    ondismiss: () => void
    copyBlocked?: boolean
    /** The owner may pause an informational notice's expiry while it is used. */
    onpause?: () => void
    onresume?: () => void
  } = $props()

  let missing = $state<string | null>(null)
  let overflowing = $state(false)
  let hovered = false
  /** The saved capture's picture: a clip's thumbnail file, or a ready URL. */
  const thumbnail = $derived(notice?.media?.src ?? (notice?.media?.thumbnail ? fileUrl(notice.media.thumbnail) : null))
  const progress = $derived(notice?.progress == null ? null : Math.min(1, Math.max(0, notice.progress)))
  const blocked = (label: string) => copyBlocked && /^copy\b/i.test(label)
  /** The key named beside an action, as aria-keyshortcuts spells it. */
  const shortcut = (key?: string) => key?.replace('Ctrl', 'Control')

  function watchMessage(node: HTMLDivElement) {
    const measure = () => (overflowing = node.scrollHeight > node.clientHeight + 1)
    const observer = new ResizeObserver(measure)
    observer.observe(node)
    if (node.firstElementChild) observer.observe(node.firstElementChild)
    measure()
    return { destroy: () => observer.disconnect() }
  }

  function enter() {
    hovered = true
    onpause?.()
  }

  function leave(event: MouseEvent) {
    hovered = false
    if (!(event.currentTarget as HTMLElement).contains(document.activeElement)) onresume?.()
  }

  function blur(event: FocusEvent) {
    const next = event.relatedTarget
    if (!hovered && !(next instanceof Node && (event.currentTarget as HTMLElement).contains(next))) onresume?.()
  }
</script>

{#if notice}
  <!-- Keep the action nodes mounted while the progress text changes, so
       keyboard focus does not disappear from Cancel or Undo. -->
  <section class="capture-notice" class:error={notice.error} out:fly={{ y: 4, duration: 120 }} aria-label="Notification" onmouseenter={enter} onmouseleave={leave} onfocusin={() => onpause?.()} onfocusout={blur}>
    <div class="notice-layout">
      {#if notice.media && thumbnail && thumbnail !== missing}
        <div class="notice-thumbnail" aria-hidden="true">
          <img src={thumbnail} alt="" draggable="false" onerror={() => (missing = thumbnail)} />
        </div>
      {/if}

      <div class="notice-body">
        <!-- Only overflowing messages enter the tab order, so keyboard users
             can scroll their full text without hiding the action buttons. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="notice-message" use:watchMessage tabindex={overflowing ? 0 : undefined} role="status" aria-live="polite" aria-atomic="true" aria-label={overflowing ? 'Notification details' : undefined}>
          <div class="notice-copy">
            <p class="notice-title">{notice.text}</p>
            {#if notice.media}
              <p class="notice-media">
                <span class="notice-game" title={gameName(notice.media.game)}>{gameName(notice.media.game)}</span>
                {#if notice.media.duration != null}<span class="notice-length">{duration(notice.media.duration)}</span>{/if}
              </p>
            {/if}
            {#if notice.detail}<p class="notice-detail">{notice.detail}</p>{/if}
          </div>
        </div>

        {#if notice.action || notice.secondaryAction}
          <div class="notice-actions">
            {#if notice.action}
              <button class={notice.media && !notice.persistent ? 'btn-sm' : 'btn-text'} disabled={blocked(notice.action.label)} onclick={notice.action.run} aria-keyshortcuts={shortcut(notice.action.key)}>{notice.action.label}</button>
              {#if notice.action.key}<span class="notice-key" aria-hidden="true">{notice.action.key}</span>{/if}
            {/if}
            {#if notice.secondaryAction}
              <button class="btn-text" disabled={blocked(notice.secondaryAction.label)} onclick={notice.secondaryAction.run} aria-keyshortcuts={shortcut(notice.secondaryAction.key)}>{notice.secondaryAction.label}</button>
              {#if notice.secondaryAction.key}<span class="notice-key" aria-hidden="true">{notice.secondaryAction.key}</span>{/if}
            {/if}
          </div>
        {/if}
      </div>

      {#if !notice.persistent}
        <button class="btn-icon-sm notice-dismiss" onclick={ondismiss} aria-label="Dismiss notification" title="Dismiss"><X size={15} /></button>
      {/if}
    </div>

    {#if progress != null}
      <div class="notice-progress" role="progressbar" aria-label="Clip preparation" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(progress * 100)}>
        <span style:transform="scaleX({progress})"></span>
      </div>
    {/if}
  </section>
{/if}

<style>
  .capture-notice {
    width: 360px;
    max-width: 100%;
    padding: 15px;
    border: 1px solid var(--color-line-strong);
    border-radius: var(--radius-overlay);
    background: var(--color-raised);
    box-shadow: 0 3px 8px rgb(0 0 0 / 22%), 0 12px 28px rgb(0 0 0 / 28%);
    /* Rises into place; leaves with a short drop (out:fly). */
    transition: opacity var(--duration-normal) var(--ease-out), translate var(--duration-normal) var(--ease-out);
    @starting-style { opacity: 0; translate: 0 8px; }
  }

  .notice-layout {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .notice-thumbnail {
    flex: none;
    width: 64px;
    height: 44px;
    overflow: hidden;
    border-radius: 4px;
    background: var(--color-canvas);
  }

  .notice-thumbnail img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .notice-body {
    flex: 1;
    min-width: 0;
  }

  .notice-title {
    font-size: var(--text-body);
    line-height: 20px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .notice-message {
    max-height: min(180px, calc(100vh - 240px));
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .error .notice-title { color: var(--color-danger); }

  .notice-media {
    display: flex;
    align-items: baseline;
    gap: 9px;
    margin-top: 3px;
    color: var(--color-fg-2);
    font-size: var(--text-secondary);
    line-height: 18px;
  }

  .notice-game {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .notice-length {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .notice-detail {
    margin-top: 4px;
    color: var(--color-fg-2);
    font-size: var(--text-secondary);
    line-height: 18px;
    overflow-wrap: anywhere;
  }

  .notice-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-top: 9px;
  }

  .notice-actions > :global(.btn-text:first-child) { margin-left: -8px; }
  .notice-key { margin: 0 6px 0 -2px; color: var(--color-muted); font-size: var(--text-caption); }

  .notice-dismiss {
    width: 24px;
    height: 24px;
    margin: -4px -5px 0 -6px;
  }

  .notice-progress {
    height: 3px;
    overflow: hidden;
    margin-top: 13px;
    background: var(--color-line-strong);
  }

  .notice-progress span {
    display: block;
    height: 100%;
    background: var(--color-accent);
    transform-origin: left;
  }

  @media (prefers-reduced-motion: reduce) {
    .capture-notice { transition: none; }
  }
</style>
