<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts" generics="Bindings extends RealmBindings | undefined">
  import type { ColnHandle, RealmBindings } from "@coln-project/repo"
  import { onDestroy, onMount } from "svelte"
  import ReplEditor from "./ReplEditor.svelte"
  import ReplOutput from "./ReplOutput.svelte"
  import { evaluate, type Evaluation } from "./evaluate.ts"
  import { loadSource, saveSource, starterSource } from "./source-storage.ts"

  let { handle, compact = false }: {
    handle: ColnHandle<Bindings>
    compact?: boolean
  } = $props()

  let source = $state(starterSource)
  let evaluation = $state<Evaluation>()
  let running = $state(false)
  let runVersion = 0

  onMount(() => {
    source = loadSource(handle.url)
  })
  onDestroy(() => {
    runVersion += 1
  })

  async function runProgram(): Promise<void> {
    if (running) return
    const version = ++runVersion
    saveSource(handle.url, source)
    running = true
    evaluation = undefined
    try {
      const result = await evaluate(source, handle)
      if (version === runVersion) evaluation = result
    } finally {
      if (version === runVersion) running = false
    }
  }
</script>

<section class="repl" data-compact={compact || undefined} data-testid="store-repl">
  <div class="pane-head">
    <div>
      <p class="label">JavaScript REPL</p>
      <p class="hint">
        Run only code you trust / <code>handle</code> is in scope / use
        <code>return</code> for the result
      </p>
    </div>
    <button
      class="action"
      data-primary
      data-pending
      disabled={running}
      onclick={runProgram}
      data-testid="run-program"
    >{running ? "Running…" : "Run ⌃↩"}</button>
  </div>
  <div class="editor-pane">
    <ReplEditor
      value={source}
      disabled={running}
      onchange={value => (source = value)}
      onrun={runProgram}
    />
  </div>
  <div class="output-pane">
    <ReplOutput {evaluation} />
  </div>
</section>

<style>
  .repl {
    display: grid;
    grid-template-rows: auto minmax(0, 2fr) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .editor-pane {
    display: flex;
    min-height: 12rem;
    overflow: hidden;
  }

  .output-pane {
    border-top: 1px solid var(--coln-border);
    min-height: 6rem;
    overflow: hidden;
  }

  .repl[data-compact] .editor-pane {
    min-height: 9rem;
  }

  code {
    font-family: var(--coln-mono);
  }
</style>
