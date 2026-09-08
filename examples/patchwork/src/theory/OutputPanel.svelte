<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Snippet } from "svelte"

  let { label, index, children }: {
    label: string
    index: string
    children: Snippet
  } = $props()
</script>

<section class="panel">
  <header class="head">
    <h2 class="label">{label}</h2>
    <span class="index">{index}</span>
  </header>
  <div class="body">{@render children()}</div>
</section>

<style>
  .panel {
    background: var(--coln-fill);
    border: 1px solid var(--coln-border);
    border-radius: var(--studio-radius-sm, 4px);
    min-width: 0;
  }

  .head {
    align-items: center;
    border-bottom: 1px solid var(--coln-border);
    display: flex;
    justify-content: space-between;
    padding: 0.625rem var(--studio-space-sm, 0.75rem);
  }

  .head h2 {
    font-size: 0.75rem;
    font-weight: 500;
  }

  .index {
    color: var(--coln-faint);
    font-family: var(--coln-mono);
    font-size: 0.75rem;
  }

  .body {
    font-family: var(--coln-mono);
    font-size: 0.875rem;
    line-height: 1.6;
    min-height: 4rem;
    overflow: auto;
    padding: var(--studio-space-sm, 0.75rem);
  }
</style>
