<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { onMount } from "svelte"
  import { loadCompiler, type Compilation, type Compiler } from "../lib/compiler.ts"
  import { loadExample, loadExampleNames } from "../lib/examples.ts"
  import JsonViewer from "./JsonViewer.svelte"
  import OutputPanel from "./OutputPanel.svelte"
  import SourceEditor from "./SourceEditor.svelte"

  const emptyCompilation: Compilation = {
    diagnosticsHtml: [],
    prettyIr: [],
    irJson: "",
  }

  let source = $state("")
  let selectedExample = $state("")
  let exampleNames = $state<string[]>([])
  let compilation = $state<Compilation>(emptyCompilation)
  let status = $state<"loading" | "loading-example" | "ready" | "compiling" | "error">("loading")
  let error = $state("")
  let compiler = $state<Compiler>()
  let compileTimer: ReturnType<typeof setTimeout> | undefined
  let compileVersion = 0
  let exampleVersion = 0
  let destroyed = false

  const sourceLines = $derived(source === "" ? 0 : source.split("\n").length)
  const statusLabel = $derived(
    status === "loading"
      ? "loading compiler"
      : status === "loading-example"
        ? "loading example"
      : status === "compiling"
        ? "compiling"
        : status === "error"
          ? "compiler error"
          : "compiler ready",
  )
  const statusColor = $derived(status === "error" ? "text-[#ff9a86]" : status === "ready" ? "text-[#d8ff57]" : "text-[#91a0a1]")
  const statusDot = $derived(status === "error" ? "bg-[#ff7657]" : status === "ready" ? "bg-[#d8ff57] shadow-[0_0_12px_#d8ff5788]" : "bg-[#819091]")

  onMount(() => {
    void initialize()
    return () => {
      destroyed = true
      clearTimeout(compileTimer)
    }
  })

  async function initialize() {
    try {
      const [loadedCompiler, loadedExamples] = await Promise.all([
        loadCompiler(),
        loadExampleNames(),
      ])
      if (destroyed) return
      compiler = loadedCompiler
      exampleNames = loadedExamples
      status = "ready"
    } catch (cause) {
      showError(cause)
    }
  }

  function updateSource(value: string) {
    ++exampleVersion
    source = value
    if (value === "") {
      clearTimeout(compileTimer)
      ++compileVersion
      compilation = emptyCompilation
      error = ""
      status = "ready"
      return
    }
    scheduleCompile(50)
  }

  async function selectExample(name: string) {
    selectedExample = name
    const version = ++exampleVersion
    clearTimeout(compileTimer)
    ++compileVersion

    if (!name) {
      source = ""
      compilation = emptyCompilation
      error = ""
      status = compiler ? "ready" : "loading"
      return
    }

    status = "loading-example"
    error = ""
    try {
      const example = await loadExample(name)
      if (destroyed || version !== exampleVersion) return
      source = example
      scheduleCompile(0)
    } catch (cause) {
      if (version === exampleVersion) showError(cause)
    }
  }

  function scheduleCompile(delay: number) {
    if (!compiler) return
    const version = ++compileVersion
    clearTimeout(compileTimer)
    status = "compiling"
    error = ""
    compileTimer = setTimeout(() => void runCompile(version), delay)
  }

  async function runCompile(version: number) {
    if (!compiler) return
    try {
      const nextCompilation = await compiler.compile(source)
      if (destroyed || version !== compileVersion) return
      compilation = nextCompilation
      status = "ready"
    } catch (cause) {
      if (version === compileVersion) showError(cause)
    }
  }

  function showError(cause: unknown) {
    if (destroyed) return
    error = cause instanceof Error ? cause.message : String(cause)
    status = "error"
  }
</script>

<svelte:head>
  <title>Coln Compiler</title>
</svelte:head>

<main class="grid min-h-screen min-w-80 grid-rows-[56px_1fr] bg-[#101718] font-['Manrope'] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
  <header class="flex items-center justify-between border-b border-[#304041] px-3.5 min-[761px]:px-6">
    <a class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] no-underline min-[761px]:tracking-[.13em]" href="/">
      <span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
      <span>COLN / COMPILER</span>
    </a>
    <div class="grid justify-items-end gap-1 font-['DM_Mono']">
      <div class={`flex items-center gap-2 text-[10px] tracking-[.08em] uppercase ${statusColor}`} role="status">
        <span class={`size-1.75 rounded-full ${statusDot}`}></span>{statusLabel}
      </div>
      <span class="text-[9px] text-[#667576]">{sourceLines} {sourceLines === 1 ? "line" : "lines"}</span>
    </div>
  </header>

  <div class="grid min-h-0 min-[761px]:grid-cols-[minmax(320px,0.9fr)_minmax(0,1.1fr)]">
    <section class="flex min-h-0 flex-col border-b border-[#304041] bg-[#101718] min-[761px]:border-r min-[761px]:border-b-0">
      <div class="flex min-h-16 items-center justify-between gap-4 border-b border-[#304041] px-4 min-[761px]:px-5">
        <div>
          <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">SOURCE</p>
          <p class="mt-1 mb-0 text-xs text-[#91a0a1]">Edit source to compile</p>
        </div>
        <label class="grid gap-1 font-['DM_Mono'] text-[9px] tracking-widest text-[#839193] uppercase" for="example">
          Example
          <select
            class="h-8 min-w-32 rounded-none border border-[#304041] bg-[#182122] px-2 font-['DM_Mono'] text-xs normal-case text-[#e8ece8] outline-none focus:border-[#d8ff57] disabled:cursor-wait disabled:opacity-50"
            id="example"
            value={selectedExample}
            disabled={!compiler}
            onchange={event => void selectExample(event.currentTarget.value)}
          >
            <option value="">—</option>
            {#each exampleNames as name}
              <option value={name}>{name.replace(/\.coln$/, "")}</option>
            {/each}
          </select>
        </label>
      </div>
      <SourceEditor value={source} disabled={!compiler} oninput={updateSource} />
    </section>

    <section class={`grid content-start gap-4 overflow-auto bg-[#131b1c] p-4 transition-opacity min-[761px]:p-6 ${status === "compiling" ? "opacity-55" : "opacity-100"}`} aria-busy={status === "compiling"}>
      {#if error}
        <div class="border-l-3 border-[#ff7657] bg-[#182122] p-4 font-['DM_Mono'] text-xs leading-relaxed text-[#ff9a86]" role="alert">
          {error}
        </div>
      {/if}

      <OutputPanel label="Diagnostics" index="01">
        {#if compilation.diagnosticsHtml.length === 0}
          <p class="m-0 text-[#667576]">No diagnostics</p>
        {:else}
          <div class="diagnostics grid gap-4">
            {#each compilation.diagnosticsHtml as diagnostic}
              <div>{@html diagnostic}</div>
            {/each}
          </div>
        {/if}
      </OutputPanel>

      <OutputPanel label="IR" index="02">
        {#if compilation.prettyIr.length === 0}
          <p class="m-0 text-[#667576]">No intermediate representation</p>
        {:else}
          <div class="grid gap-4">
            {#each compilation.prettyIr as realm}
              <pre class="m-0 whitespace-pre-wrap wrap-break-word">{realm}</pre>
            {/each}
          </div>
        {/if}
      </OutputPanel>

      <OutputPanel label="JSON" index="03">
        {#if compilation.irJson === ""}
          <p class="m-0 text-[#667576]">No JSON output</p>
        {:else}
          <JsonViewer value={compilation.irJson} />
        {/if}
      </OutputPanel>
    </section>
  </div>
</main>
