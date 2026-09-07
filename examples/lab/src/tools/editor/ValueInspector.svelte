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

  function renderValue(node: HTMLElement, initialValue: Snapshot) {
    function render(nextValue: Snapshot) {
      formatter = new JSONFormatter(nextValue, 2, {
        animateClose: false,
        animateOpen: false,
        hoverPreviewEnabled: true,
        theme: "dark",
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

<div class="lab-json min-w-0" data-testid="value-inspector">
  <div class="mb-1 flex items-center justify-end gap-2">
    {#if structured}
      <button class="cursor-pointer border-0 bg-transparent p-0 text-sm text-[#839193] hover:text-[#d8ff57]" type="button" onclick={() => formatter?.openAtDepth(Infinity)}>Expand all</button>
      <button class="cursor-pointer border-0 bg-transparent p-0 text-sm text-[#839193] hover:text-[#d8ff57]" type="button" onclick={() => formatter?.openAtDepth(0)}>Collapse all</button>
    {/if}
    <button class="cursor-pointer border-0 bg-transparent p-0 text-sm text-[#839193] hover:text-[#d8ff57]" type="button" onclick={copyValue}>{copyLabel}</button>
  </div>

  {#if structured}
    <div class="min-w-max" use:renderValue={value}></div>
  {:else}
    <code class={typeof value === "string" ? "text-[#d8ff57]" : typeof value === "number" ? "text-[#ffd166]" : typeof value === "boolean" ? "text-[#ff9a86]" : "text-[#bba4ff]"}>{primitive}</code>
  {/if}
</div>
