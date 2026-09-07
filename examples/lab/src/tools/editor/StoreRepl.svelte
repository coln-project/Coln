<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts" generics="Bindings extends RealmBindings | undefined">
  import type { ColnHandle, RealmBindings } from "@coln-project/repo"
  import { Pane, PaneGroup } from "paneforge"
  import { onDestroy, onMount } from "svelte"
  import LabPaneResizer from "../../lib/LabPaneResizer.svelte"
  import ReplEditor from "./ReplEditor.svelte"
  import ReplOutput from "./ReplOutput.svelte"
  import { evaluate, type Evaluation } from "./evaluate.ts"
  import { loadSource, saveSource, starterSource } from "./source-storage.ts"

  let { handle, active = true, compact = false, layoutId }: {
    handle: ColnHandle<Bindings>
    active?: boolean
    compact?: boolean
    layoutId: string
  } = $props()

  let source = $state(starterSource)
  let evaluation = $state<Evaluation>()
  let running = $state(false)
  let runVersion = 0

  onMount(() => source = loadSource(handle.url))
  onDestroy(() => runVersion += 1)

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

<section class:hidden={!active} class={`h-full min-h-0 flex-1 flex-col bg-[#131b1c] ${active ? "flex" : ""}`} data-testid="store-repl">
  <div class={`flex items-center justify-between gap-4 border-b border-[#304041] px-4 min-[761px]:px-5 ${compact ? "min-h-14" : "min-h-16"}`}>
    <div>
      <p class="m-0 font-['DM_Mono'] text-xs tracking-[.16em] text-[#748284]" data-small-detail>JAVASCRIPT REPL</p>
      <p class="mt-1 mb-0 text-sm text-[#91a0a1]">Run only code you trust / handle is in scope / use return for the result</p>
    </div>
    <button class="lab-primary-action lab-pending-action h-9 shrink-0 px-4 font-['DM_Mono'] text-sm font-medium tracking-[.12em] uppercase" disabled={running} onclick={runProgram} data-testid="run-program">{running ? "Running…" : "Run ⌘↵"}</button>
  </div>
  <PaneGroup class="lab-repl-pane-group min-h-0 flex-1" direction="vertical" autoSaveId={layoutId}>
    <Pane id={`${layoutId}-editor-pane`} class="flex min-h-0 flex-col" defaultSize={72} minSize={45} maxSize={85}>
      <ReplEditor value={source} disabled={running} {active} onchange={(value) => (source = value)} onrun={runProgram} />
    </Pane>
    <LabPaneResizer label="Resize JavaScript editor and output" orientation="horizontal" testId="repl-output-resizer" />
    <Pane id={`${layoutId}-output-pane`} class="min-h-0" defaultSize={28} minSize={15} maxSize={55}>
      <ReplOutput {evaluation} />
    </Pane>
  </PaneGroup>
</section>
