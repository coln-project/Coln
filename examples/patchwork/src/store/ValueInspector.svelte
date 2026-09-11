<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import JSONFormatter from "json-formatter-js"
  import { onDestroy } from "svelte"
  import type { Snapshot } from "./snapshot.ts"

  let { value }: { value: Snapshot } = $props()

  let formatter: JSONFormatter | undefined
  let copyLabel = $state("Copy value")
  let copyTimer: ReturnType<typeof setTimeout> | undefined

  const structured = $derived(value !== null && typeof value === "object")
  const primitive = $derived(JSON.stringify(value))
  const primitiveKind = $derived(
    typeof value === "string"
      ? "string"
      : typeof value === "number"
        ? "number"
        : typeof value === "boolean"
          ? "boolean"
          : "null",
  )

  function renderValue(node: HTMLElement, initialValue: Snapshot) {
    function render(nextValue: Snapshot) {
      formatter = new JSONFormatter(nextValue, 2, {
        animateClose: false,
        animateOpen: false,
        hoverPreviewEnabled: true,
        useToJSON: false,
      })
      node.replaceChildren(formatter.render())
    }

    render(initialValue)
    return { update: render }
  }

  async function copyValue(): Promise<void> {
    try {
      await navigator.clipboard.writeText(JSON.stringify(value, null, 2))
      copyLabel = "Value copied"
    } catch {
      copyLabel = "Copy failed"
    }
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => (copyLabel = "Copy value"), 2_000)
  }

  onDestroy(() => clearTimeout(copyTimer))
</script>

<div class="coln-json inspector" data-testid="value-inspector">
  <div class="controls">
    {#if structured}
      <button class="link" type="button" onclick={() => formatter?.openAtDepth(Infinity)}>Expand all</button>
      <button class="link" type="button" onclick={() => formatter?.openAtDepth(0)}>Collapse all</button>
    {/if}
    <button class="link" type="button" onclick={copyValue}>{copyLabel}</button>
  </div>

  {#if structured}
    <div class="tree" use:renderValue={value}></div>
  {:else}
    <code class="primitive" data-kind={primitiveKind}>{primitive}</code>
  {/if}
</div>

<style>
  .inspector {
    min-width: 0;
  }

  .controls {
    display: flex;
    gap: var(--studio-space-xs, 0.375rem);
    justify-content: flex-end;
    margin-bottom: 0.25rem;
  }

  .link {
    background: none;
    border: 0;
    color: var(--coln-muted);
    cursor: pointer;
    font-family: var(--coln-mono);
    font-size: 0.8125rem;
    padding: 0;
  }

  .link:hover {
    color: var(--coln-accent-text);
  }

  .tree {
    min-width: max-content;
  }

  .primitive {
    font-family: var(--coln-mono);
    font-size: 0.875rem;
  }

  .primitive[data-kind="string"] { color: var(--coln-accent-text); }
  .primitive[data-kind="number"] { color: var(--coln-warning-text); }
  .primitive[data-kind="boolean"] { color: var(--coln-danger-text); }
  .primitive[data-kind="null"] { color: var(--coln-faint); }
</style>
