<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import {
    isValidAutomergeUrl,
    type AutomergeUrl,
    type Repo,
  } from "@automerge/automerge-repo"
  import { onDestroy, onMount } from "svelte"
  import JsonViewer from "./JsonViewer.svelte"
  import OutputPanel from "./OutputPanel.svelte"
  import SourceEditor from "./SourceEditor.svelte"
  import StoresPanel from "./StoresPanel.svelte"
  import {
    parseCompiledRealms,
    type CompiledRealm,
  } from "./compiled-realms.ts"
  import {
    loadCompiler,
    type Compilation,
    type Compiler,
  } from "./compiler.ts"
  import { createStore } from "./stores.ts"
  import {
    isTheoryDocument,
    newTheoryDocument,
    type TheoryDocument,
    type TheoryDocumentHandle,
  } from "./theory-document.ts"
  import { TheoryHandle } from "./theory-handle.svelte.ts"
  import { DocumentSync } from "../../lib/document-sync.svelte.ts"
  import type { Router } from "../../lib/router.svelte.ts"

  type LoadError = "invalid" | "unavailable" | "incompatible"

  let { repo, endpoint, router, documentUrl, trackDocument }: {
    repo: Repo
    endpoint: string
    router: Router
    documentUrl: string
    trackDocument: (documentId: string) => void
  } = $props()

  const emptyCompilation: Compilation = {
    diagnosticsHtml: [],
    prettyIr: [],
    irJson: "",
  }

  let handle = $state<TheoryDocumentHandle>()
  let theoryHandle = $state<TheoryHandle>()
  let sync = $state<DocumentSync>()
  let loadError = $state<LoadError>()
  let compilation = $state<Compilation>(emptyCompilation)
  let status = $state<"loading" | "ready" | "compiling" | "error">("loading")
  let error = $state("")
  let compiler = $state<Compiler>()
  let compiledSource = $state("")
  let realms = $state<CompiledRealm[]>([])
  let selectedRealmName = $state("")
  let creatingStore = $state(false)
  let storeError = $state("")
  let feedback = $state("")
  let copyPending = $state(false)
  let compileTimer: ReturnType<typeof setTimeout> | undefined
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let removeDocumentListener: (() => void) | undefined
  let compileVersion = 0
  let destroyed = false

  const theory = $derived(theoryHandle?.state ?? newTheoryDocument())
  const syncStatus = $derived(sync?.status ?? "offline")
  const syncError = $derived(
    sync?.error instanceof Error ? sync.error.message : String(sync?.error ?? ""),
  )
  const sourceLines = $derived(
    theory.source === "" ? 0 : theory.source.split("\n").length,
  )
  const statusLabel = $derived(
    status === "loading"
      ? "loading compiler"
      : status === "compiling"
        ? "compiling"
        : status === "error"
          ? "compiler error"
          : "compiler ready",
  )
  const statusColor = $derived(
    status === "error"
      ? "text-[#ff9a86]"
      : status === "ready"
        ? "text-[#d8ff57]"
        : "text-[#91a0a1]",
  )
  const statusDot = $derived(
    status === "error"
      ? "bg-[#ff7657]"
      : status === "ready"
        ? "bg-[#d8ff57] shadow-[0_0_12px_#d8ff5788]"
        : "bg-[#819091]",
  )
  const syncColor = $derived(
    syncStatus === "error"
      ? "text-[#ff9a86]"
      : syncStatus === "synced"
        ? "text-[#d8ff57]"
        : "text-[#819091]",
  )
  const syncDot = $derived(
    syncStatus === "error"
      ? "bg-[#ff7657]"
      : syncStatus === "synced"
        ? "bg-[#d8ff57]"
        : "bg-[#819091]",
  )
  const selectedRealm = $derived(
    realms.find((realm) => realm.name === selectedRealmName),
  )
  const stores = $derived([...theory.stores].reverse())
  const canCreateStore = $derived(
    status === "ready" &&
      compilation.diagnosticsHtml.length === 0 &&
      compiledSource === theory.source &&
      selectedRealm !== undefined &&
      !creatingStore,
  )
  const createStoreHint = $derived(
    creatingStore
      ? "Flushing store before adding it to this theory."
      : status === "compiling" || compiledSource !== theory.source
        ? "Waiting for the current source to compile."
        : compilation.diagnosticsHtml.length > 0
          ? "Resolve diagnostics before creating a store."
          : realms.length > 1 && !selectedRealm
            ? "Choose a realm to create."
            : "",
  )

  onMount(() => {
    if (documentUrl && !isValidAutomergeUrl(documentUrl)) {
      loadError = "invalid"
      return
    }
    void loadTheory()
  })

  onDestroy(() => {
    destroyed = true
    compileVersion += 1
    clearTimeout(compileTimer)
    clearTimeout(feedbackTimer)
    removeDocumentListener?.()
  })

  async function loadTheory(): Promise<void> {
    try {
      const loaded = documentUrl
        ? await repo.find<TheoryDocument>(documentUrl as AutomergeUrl)
        : repo.create(newTheoryDocument())
      if (destroyed) return
      if (!isTheoryDocument(loaded.doc())) {
        loadError = "incompatible"
        return
      }
      if (!documentUrl) {
        router.replace("compiler", loaded.url)
        return
      }

      handle = loaded
      trackDocument(loaded.documentId)
      theoryHandle = new TheoryHandle(loaded)
      sync = new DocumentSync(repo, loaded)
      let currentSource = loaded.doc().source
      const documentChanged = () => {
        const nextSource = loaded.doc().source
        if (nextSource === currentSource) return
        currentSource = nextSource
        sourceChanged(nextSource, 50)
      }
      loaded.on("change", documentChanged)
      removeDocumentListener = () => loaded.off("change", documentChanged)
      void initializeCompiler()
    } catch (cause) {
      if (!destroyed) loadError = isUnavailable(cause) ? "unavailable" : "incompatible"
    }
  }

  async function initializeCompiler(): Promise<void> {
    try {
      const loadedCompiler = await loadCompiler()
      if (destroyed || !handle) return
      compiler = loadedCompiler
      sourceChanged(handle.doc().source, 0)
    } catch (cause) {
      showError(cause)
    }
  }

  function sourceChanged(source: string, delay: number): void {
    clearTimeout(compileTimer)
    const version = ++compileVersion
    error = ""
    storeError = ""
    compiledSource = ""
    realms = []
    if (source === "") {
      compilation = emptyCompilation
      selectedRealmName = ""
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

  async function runCompile(version: number, source: string): Promise<void> {
    if (!compiler) return
    try {
      const nextCompilation = await compiler.compile(source)
      if (destroyed || version !== compileVersion) return
      const nextRealms = parseCompiledRealms(nextCompilation.irJson)
      compilation = nextCompilation
      compiledSource = source
      realms = nextRealms
      selectedRealmName =
        nextRealms.length === 1
          ? nextRealms[0].name
          : nextRealms.some((realm) => realm.name === selectedRealmName)
            ? selectedRealmName
            : ""
      status = "ready"
    } catch (cause) {
      if (version === compileVersion) showError(cause)
    }
  }

  function showError(cause: unknown): void {
    if (destroyed) return
    error = cause instanceof Error ? cause.message : String(cause)
    status = "error"
  }

  async function createSelectedStore(): Promise<void> {
    if (!canCreateStore || !selectedRealm || !theoryHandle) return
    creatingStore = true
    storeError = ""
    try {
      const { record, theoryFlushError } = await createStore(
        repo,
        theoryHandle,
        selectedRealm,
      )
      showFeedback(`${record.realmName} store created`)
      if (theoryFlushError) {
        const detail =
          theoryFlushError instanceof Error
            ? theoryFlushError.message
            : String(theoryFlushError)
        storeError = `Store created, but its theory record was not flushed: ${detail}`
      }
    } catch (cause) {
      storeError = cause instanceof Error ? cause.message : String(cause)
    } finally {
      creatingStore = false
    }
  }

  async function copyText(value: string, message: string): Promise<void> {
    await navigator.clipboard.writeText(value)
    showFeedback(message)
  }

  function showFeedback(message: string): void {
    feedback = message
    clearTimeout(feedbackTimer)
    feedbackTimer = setTimeout(() => (feedback = ""), 2_000)
  }

  function isUnavailable(cause: unknown): boolean {
    return cause instanceof Error && /^Document .+ is unavailable$/.test(cause.message)
  }

  const errorCopy = {
    invalid: ["DOCUMENT URL INVALID", "This is not a valid Automerge document URL.", "Check the URL and try again, or create a new theory."],
    unavailable: ["DOCUMENT UNAVAILABLE", "This theory could not be found.", "It may not have synchronized yet, or may no longer be available."],
    incompatible: ["DOCUMENT INCOMPATIBLE", "This document is not a supported Coln theory.", "Its structure or version does not match this compiler."],
  } satisfies Record<LoadError, [string, string, string]>
</script>

{#if loadError}
  <section class="grid h-full place-items-center bg-[#101718] p-6">
    <div class="grid w-full max-w-xl gap-6 border border-[#304041] bg-[#182122] p-6 min-[601px]:p-10">
      <div class="grid gap-3 border-l-3 border-[#ff7657] pl-5">
        <p class="m-0 font-['DM_Mono'] text-xs tracking-[.14em] text-[#ff9a86]">{errorCopy[loadError][0]}</p>
        <h1 class="m-0 text-3xl font-semibold tracking-[-.03em]">{errorCopy[loadError][1]}</h1>
        <p class="m-0 max-w-md leading-7 text-[#aab6b6]">{errorCopy[loadError][2]}</p>
      </div>
      {#if documentUrl}<code class="overflow-hidden text-ellipsis border border-[#304041] bg-[#101718] p-3 font-['DM_Mono'] text-xs text-[#839193]">{documentUrl}</code>{/if}
      <button class="flex h-12 cursor-pointer items-center justify-between border-0 bg-[#d8ff57] px-4 font-bold text-[#101718]" onclick={() => router.navigate("compiler")}>Create a new theory <span class="font-['DM_Mono'] text-xl">+</span></button>
    </div>
  </section>
{:else if handle && theoryHandle}
  {@const currentTheoryUrl = handle.url}
  <section class="relative grid h-full min-h-0 grid-rows-[56px_1fr] bg-[#101718] min-[761px]:grid-rows-[64px_1fr]">
    {#if feedback}<p class="absolute top-20 left-1/2 z-20 m-0 -translate-x-1/2 border border-[#d8ff57] bg-[#182122] px-4 py-2 font-['DM_Mono'] text-xs font-medium text-[#d8ff57] shadow-[0_8px_24px_#0008]" role="status">{feedback}</p>{/if}
    <header class="flex min-w-0 items-center justify-between gap-3 border-b border-[#304041] px-3.5 min-[761px]:px-6">
      <button class="flex shrink-0 cursor-pointer items-center gap-3 border-0 bg-transparent p-0 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] min-[761px]:tracking-[.13em]" aria-label="Create a new theory" onclick={() => router.navigate("compiler")}><span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span><span class="hidden min-[470px]:inline">COLN / COMPILER</span></button>
      <div class="flex items-center gap-3 min-[761px]:gap-5">
        <button class="h-8 cursor-pointer border border-[#6b7a7b] bg-transparent px-2.5 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#e8ece8] uppercase hover:border-[#d8ff57] hover:text-[#d8ff57] disabled:cursor-wait disabled:opacity-40" disabled={copyPending} onclick={async () => { copyPending = true; try { await copyText(location.href, "Project URL copied") } catch (cause) { showError(cause) } finally { copyPending = false } }}>Copy URL</button>
        <div class="grid justify-items-end gap-1 font-['DM_Mono']">
          <div class={`flex items-center gap-2 text-[10px] tracking-[.08em] uppercase ${statusColor}`} role="status"><span class={`size-1.75 rounded-full ${statusDot}`}></span>{statusLabel}</div>
          <div class={`flex items-center gap-2 text-[9px] uppercase ${syncColor}`} title={syncStatus === "error" ? syncError : endpoint}><span class={`size-1.5 rounded-full ${syncDot}`}></span>{syncStatus}<span class="text-[#667576]">/ {sourceLines} {sourceLines === 1 ? "line" : "lines"}</span></div>
        </div>
      </div>
    </header>
    <div class="grid min-h-0 overflow-auto min-[761px]:grid-cols-[minmax(320px,0.9fr)_minmax(0,1.1fr)] min-[1100px]:grid-cols-[minmax(320px,0.8fr)_minmax(320px,1fr)_300px] min-[761px]:overflow-hidden">
       <section class="flex min-h-0 flex-col border-b border-[#304041] bg-[#101718] min-[761px]:border-r min-[761px]:border-b-0"><div class="flex min-h-16 items-center justify-between border-b border-[#304041] px-5"><div><p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">SOURCE</p><p class="mt-1 mb-0 text-xs text-[#91a0a1]">Collaborative Automerge document</p></div><span class="font-['DM_Mono'] text-[9px] text-[#839193] uppercase">Live theory</span></div><SourceEditor {handle} /></section>
       <section class={`grid content-start gap-4 overflow-auto bg-[#131b1c] p-4 transition-opacity min-[761px]:p-6 ${status === "compiling" ? "opacity-55" : "opacity-100"}`} aria-busy={status === "compiling"}>
         {#if error}<div class="border-l-3 border-[#ff7657] bg-[#182122] p-4 font-['DM_Mono'] text-xs leading-relaxed text-[#ff9a86]" role="alert">{error}</div>{/if}
         {#if syncStatus === "error"}<div class="border-l-3 border-[#ff7657] bg-[#182122] p-4 font-['DM_Mono'] text-xs leading-relaxed text-[#ff9a86]" role="alert">Sync error: {syncError}</div>{/if}
         <OutputPanel label="Diagnostics" index="01">{#if compilation.diagnosticsHtml.length === 0}<p class="m-0 text-[#667576]">No diagnostics</p>{:else}<div class="diagnostics grid gap-4">{#each compilation.diagnosticsHtml as diagnostic}<div>{@html diagnostic}</div>{/each}</div>{/if}</OutputPanel>
        <OutputPanel label="IR" index="02">{#if compilation.prettyIr.length === 0}<p class="m-0 text-[#667576]">No intermediate representation</p>{:else}<div class="grid gap-4">{#each compilation.prettyIr as realm}<pre class="m-0 whitespace-pre-wrap wrap-break-word">{realm}</pre>{/each}</div>{/if}</OutputPanel>
        <OutputPanel label="JSON" index="03">{#if compilation.irJson === ""}<p class="m-0 text-[#667576]">No JSON output</p>{:else}<JsonViewer value={compilation.irJson} />{/if}</OutputPanel>
      </section>
      <StoresPanel {stores} {realms} {selectedRealmName} canCreate={canCreateStore} creating={creatingStore} error={storeError} hint={createStoreHint} onselect={(name) => (selectedRealmName = name)} oncreate={createSelectedStore} oncopy={(url) => void copyText(url, "Store URL copied")} openhref={(url) => router.href("editor", url, currentTheoryUrl)} onopen={(event, url) => router.follow(event, "editor", url, currentTheoryUrl)} />
    </div>
  </section>
{:else}
   <section class="grid h-full place-items-center bg-[#101718]"><p class="font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">LOADING COMPILER</p></section>
{/if}
