<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Repo } from "@automerge/automerge-repo"
  import { onMount } from "svelte"
  import { loadCompiler, type Compilation, type Compiler } from "../lib/compiler.ts"
  import { TheoryHandle } from "../lib/theory-handle.svelte.ts"
  import { TheorySync } from "../lib/theory-sync.svelte.ts"
  import type { TheoryDocumentHandle } from "../lib/theory-document.ts"
  import JsonViewer from "./JsonViewer.svelte"
  import OutputPanel from "./OutputPanel.svelte"
  import SourceEditor from "./SourceEditor.svelte"

  let { repo, handle, endpoint }: {
    repo: Repo
    handle: TheoryDocumentHandle
    endpoint: string
  } = $props()

  const emptyCompilation: Compilation = {
    diagnosticsHtml: [],
    prettyIr: [],
    irJson: "",
  }

  const theoryHandle = $derived(new TheoryHandle(handle))
  const sync = $derived(new TheorySync(repo, handle))

  let compilation = $state<Compilation>(emptyCompilation)
  let status = $state<"loading" | "ready" | "compiling" | "error">("loading")
  let error = $state("")
  let compiler = $state<Compiler>()
  let feedback = $state("")
  let copyPending = $state(false)
  let compileTimer: ReturnType<typeof setTimeout> | undefined
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let compileVersion = 0
  let destroyed = false

  const theory = $derived(theoryHandle.state)
  const sourceLines = $derived(theory.source === "" ? 0 : theory.source.split("\n").length)
  const syncStatus = $derived(sync.status)
  const syncError = $derived(sync.error instanceof Error ? sync.error.message : String(sync.error ?? ""))
  const statusLabel = $derived(
    status === "loading"
      ? "loading compiler"
      : status === "compiling"
        ? "compiling"
        : status === "error"
          ? "compiler error"
          : "compiler ready",
  )
  const statusColor = $derived(status === "error" ? "text-[#ff9a86]" : status === "ready" ? "text-[#d8ff57]" : "text-[#91a0a1]")
  const statusDot = $derived(status === "error" ? "bg-[#ff7657]" : status === "ready" ? "bg-[#d8ff57] shadow-[0_0_12px_#d8ff5788]" : "bg-[#819091]")
  const syncColor = $derived(syncStatus === "error" ? "text-[#ff9a86]" : syncStatus === "synced" ? "text-[#d8ff57]" : "text-[#819091]")
  const syncDot = $derived(syncStatus === "error" ? "bg-[#ff7657]" : syncStatus === "synced" ? "bg-[#d8ff57]" : "bg-[#819091]")

  onMount(() => {
    let currentSource = handle.doc().source
    const documentChanged = () => {
      const nextSource = handle.doc().source
      if (nextSource === currentSource) return
      currentSource = nextSource
      sourceChanged(nextSource, 50)
    }
    handle.on("change", documentChanged)
    void initialize()
    return () => {
      destroyed = true
      clearTimeout(compileTimer)
      clearTimeout(feedbackTimer)
      handle.off("change", documentChanged)
    }
  })

  async function initialize() {
    try {
      const loadedCompiler = await loadCompiler()
      if (destroyed) return
      compiler = loadedCompiler
      sourceChanged(handle.doc().source, 0)
    } catch (cause) {
      showError(cause)
    }
  }

  function sourceChanged(source: string, delay: number) {
    clearTimeout(compileTimer)
    const version = ++compileVersion
    error = ""

    if (source === "") {
      compilation = emptyCompilation
      status = compiler ? "ready" : "loading"
      return
    }

    if (!compiler) {
      status = "loading"
      return
    }
    status = "compiling"
    compileTimer = setTimeout(() => void runCompile(version, source), delay)
  }

  async function runCompile(version: number, source: string) {
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

  async function copyProjectUrl() {
    if (copyPending) return
    copyPending = true
    try {
      await navigator.clipboard.writeText(location.href)
      feedback = "Project URL copied"
      clearTimeout(feedbackTimer)
      feedbackTimer = setTimeout(() => feedback = "", 2_000)
    } catch (cause) {
      showError(cause)
    } finally {
      copyPending = false
    }
  }

  function createNewTheory(event: MouseEvent) {
    event.preventDefault()
    history.replaceState(null, "", `${location.pathname}${location.search}`)
    location.reload()
  }
</script>

<svelte:head>
  <title>Coln Compiler</title>
</svelte:head>

<main class="grid min-h-screen min-w-80 grid-rows-[56px_1fr] bg-[#101718] font-['Manrope'] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
  {#if feedback}
    <p class="fixed top-20 left-1/2 z-20 m-0 -translate-x-1/2 border border-[#d8ff57] bg-[#182122] px-4 py-2 font-['DM_Mono'] text-xs font-medium text-[#d8ff57] shadow-[0_8px_24px_#0008]" role="status">
      {feedback}
    </p>
  {/if}

  <header class="flex items-center justify-between border-b border-[#304041] px-3.5 min-[761px]:px-6">
    <a class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] no-underline min-[761px]:tracking-[.13em]" href="/" aria-label="Create a new theory" onclick={createNewTheory}>
      <span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
      <span>COLN / COMPILER</span>
    </a>
    <div class="flex items-center gap-3 min-[761px]:gap-5">
      <button class="h-8 cursor-pointer border border-[#6b7a7b] bg-transparent px-2.5 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#e8ece8] uppercase hover:border-[#d8ff57] hover:text-[#d8ff57] disabled:cursor-wait disabled:opacity-40" disabled={copyPending} onclick={copyProjectUrl}>
        Copy URL
      </button>
      <div class="grid justify-items-end gap-1 font-['DM_Mono']">
        <div class={`flex items-center gap-2 text-[10px] tracking-[.08em] uppercase ${statusColor}`} role="status">
          <span class={`size-1.75 rounded-full ${statusDot}`}></span>{statusLabel}
        </div>
        <div class={`flex items-center gap-2 text-[9px] uppercase ${syncColor}`} title={syncStatus === "error" ? syncError : endpoint}>
          <span class={`size-1.5 rounded-full ${syncDot}`}></span>{syncStatus}
          <span class="text-[#667576]">/ {sourceLines} {sourceLines === 1 ? "line" : "lines"}</span>
        </div>
      </div>
    </div>
  </header>

  <div class="grid min-h-0 min-[761px]:grid-cols-[minmax(320px,0.9fr)_minmax(0,1.1fr)]">
    <section class="flex min-h-0 flex-col border-b border-[#304041] bg-[#101718] min-[761px]:border-r min-[761px]:border-b-0">
      <div class="flex min-h-16 items-center justify-between gap-4 border-b border-[#304041] px-4 min-[761px]:px-5">
        <div>
          <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">SOURCE</p>
          <p class="mt-1 mb-0 text-xs text-[#91a0a1]">Collaborative Automerge document</p>
        </div>
        <span class="font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#839193] uppercase">Live theory</span>
      </div>
      <SourceEditor {handle} />
    </section>

    <section class={`grid content-start gap-4 overflow-auto bg-[#131b1c] p-4 transition-opacity min-[761px]:p-6 ${status === "compiling" ? "opacity-55" : "opacity-100"}`} aria-busy={status === "compiling"}>
      {#if error}
        <div class="border-l-3 border-[#ff7657] bg-[#182122] p-4 font-['DM_Mono'] text-xs leading-relaxed text-[#ff9a86]" role="alert">
          {error}
        </div>
      {/if}
      {#if syncStatus === "error"}
        <div class="border-l-3 border-[#ff7657] bg-[#182122] p-4 font-['DM_Mono'] text-xs leading-relaxed text-[#ff9a86]" role="alert">
          Sync error: {syncError}
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
