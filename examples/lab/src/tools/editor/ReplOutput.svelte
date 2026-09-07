<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Evaluation } from "./evaluate.ts"
  import ValueInspector from "./ValueInspector.svelte"

  let { evaluation }: { evaluation: Evaluation | undefined } = $props()
</script>

<div class="grid h-full min-h-0 content-start gap-3 overflow-auto bg-[#131b1c] p-4 font-['DM_Mono'] text-sm leading-relaxed" data-testid="repl-output">
  {#if !evaluation}
    <p class="m-0 text-[#667576]">Run a program to see its result and console output.</p>
  {:else}
    {#each evaluation.console as entry}
      <div class={`grid grid-cols-[52px_minmax(0,1fr)] gap-3 ${entry.level === "error" ? "text-[#ff9a86]" : entry.level === "warn" ? "text-[#ffd166]" : "text-[#aab6b7]"}`} data-testid="console-entry">
        <span class="text-sm uppercase opacity-65">{entry.level}</span>
        <div class="grid min-w-0 gap-2 overflow-auto">
          {#each entry.values as value}
            <ValueInspector {value} />
          {/each}
        </div>
      </div>
    {/each}
    <div class={`border-l-2 p-3 ${evaluation.ok ? "border-[#d8ff57] bg-[#182122] text-[#e8ece8]" : "border-[#ff7657] bg-[#211918] text-[#ff9a86]"}`} data-testid={evaluation.ok ? "repl-result" : "repl-error"}>
      <div class="mb-2 flex justify-between text-sm tracking-[.12em] uppercase opacity-65">
        <span>{evaluation.ok ? "Result" : `${evaluation.phase} error`}</span>
        <span>{evaluation.durationMs.toFixed(1)} ms</span>
      </div>
      <div class="overflow-auto">
        <ValueInspector value={evaluation.ok ? evaluation.result : evaluation.error} />
      </div>
    </div>
  {/if}
</div>
