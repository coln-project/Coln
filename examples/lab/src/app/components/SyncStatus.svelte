<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { SyncStatus as Status } from "../../lib/document-sync.svelte.ts"

  let {
    status,
    detail = "",
    title = "",
    compact = false,
  }: {
    status: Status
    detail?: string
    title?: string
    compact?: boolean
  } = $props()

  const label = $derived(status === "error" ? "sync failed" : status)
</script>

<div class="sync-status flex items-center gap-2 font-['DM_Mono'] text-sm tracking-[.06em] uppercase" data-compact={compact || undefined} data-status={status} data-testid="sync-status" {title} role="status" aria-live="polite">
  <span class="sync-dot size-1.5 rounded-full"></span>{label}{#if detail}<span class="text-[#667576]">/ {detail}</span>{/if}
</div>

<style>
  .sync-status { color: var(--lab-text-muted); }
  .sync-dot { background: #819091; }
  .sync-status[data-status="synced"] { color: var(--lab-accent); }
  .sync-status[data-status="synced"] .sync-dot {
    background: var(--lab-accent);
    box-shadow: 0 0 10px #d8ff5788;
  }
  .sync-status[data-status="error"] { color: var(--lab-error); }
  .sync-status[data-status="error"] .sync-dot { background: var(--lab-error-strong); }
</style>
