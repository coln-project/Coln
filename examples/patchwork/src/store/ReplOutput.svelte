<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Evaluation } from "./evaluate.ts"
  import ValueInspector from "./ValueInspector.svelte"

  let { evaluation }: { evaluation: Evaluation | undefined } = $props()
</script>

<div class="output" data-testid="repl-output">
  {#if !evaluation}
    <p class="empty">Run a program to see its result and console output.</p>
  {:else}
    {#each evaluation.console as entry}
      <div class="entry" data-level={entry.level} data-testid="console-entry">
        <span class="level">{entry.level}</span>
        <div class="values">
          {#each entry.values as value}
            <ValueInspector {value} />
          {/each}
        </div>
      </div>
    {/each}
    <div
      class="result"
      data-ok={evaluation.ok || undefined}
      data-testid={evaluation.ok ? "repl-result" : "repl-error"}
    >
      <div class="result-head">
        <span>{evaluation.ok ? "Result" : `${evaluation.phase} error`}</span>
        <span>{evaluation.durationMs.toFixed(1)} ms</span>
      </div>
      <div class="result-body">
        <ValueInspector value={evaluation.ok ? evaluation.result : evaluation.error} />
      </div>
    </div>
  {/if}
</div>

<style>
  .output {
    align-content: start;
    background: var(--coln-surface);
    display: grid;
    font-family: var(--coln-mono);
    font-size: 0.875rem;
    gap: var(--studio-space-sm, 0.75rem);
    height: 100%;
    line-height: 1.6;
    min-height: 0;
    overflow: auto;
    padding: var(--studio-space-sm, 0.75rem);
  }

  .entry {
    color: var(--coln-muted);
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
    grid-template-columns: 3.25rem minmax(0, 1fr);
  }

  .entry[data-level="warn"] { color: var(--coln-warning-text); }
  .entry[data-level="error"] { color: var(--coln-danger-text); }

  .level {
    font-size: 0.8125rem;
    opacity: 0.65;
    text-transform: uppercase;
  }

  .values {
    display: grid;
    gap: var(--studio-space-xs, 0.375rem);
    min-width: 0;
    overflow: auto;
  }

  .result {
    background: var(--coln-alert-fill);
    border-left: 2px solid var(--coln-danger);
    color: var(--coln-danger-text);
    padding: var(--studio-space-sm, 0.75rem);
  }

  .result[data-ok] {
    background: var(--coln-fill);
    border-left-color: var(--coln-accent);
    color: var(--coln-line);
  }

  .result-head {
    display: flex;
    font-size: 0.8125rem;
    justify-content: space-between;
    letter-spacing: 0.1em;
    margin-bottom: var(--studio-space-xs, 0.375rem);
    opacity: 0.65;
    text-transform: uppercase;
  }

  .result-body {
    overflow: auto;
  }
</style>
