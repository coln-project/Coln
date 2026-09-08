<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { SyncStatus } from "../coln/sync.svelte.ts"

  let { status, detail = "", title = "" }: {
    status: SyncStatus
    detail?: string
    title?: string
  } = $props()

  const label = $derived(status === "error" ? "sync failed" : status)
</script>

<div
  class="status"
  data-state={status}
  data-testid="sync-status"
  {title}
  role="status"
  aria-live="polite"
>
  {label}{#if detail}<span class="detail">/ {detail}</span>{/if}
</div>

<style>
  .detail {
    color: var(--coln-faint);
  }
</style>
