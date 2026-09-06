<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import {
    isValidAutomergeUrl,
    type AutomergeUrl,
    type Repo,
  } from "@automerge/automerge-repo"
  import { find, type ColnHandle } from "@coln-project/repo"
  import type { RowView } from "@coln-project/runtime"
  import { onDestroy, onMount } from "svelte"
  import ReplEditor from "./ReplEditor.svelte"
  import ReplOutput from "./ReplOutput.svelte"
  import TableBrowser from "./TableBrowser.svelte"
  import { ColnHandleState } from "./coln-handle.svelte.ts"
  import {
    evaluate,
    type Evaluation,
  } from "./evaluate.ts"
  import {
    readTables,
    type StoreTable,
  } from "./schema.ts"
  import {
    loadSource,
    saveSource,
  } from "./source-storage.ts"
  import { DocumentSync } from "../../lib/document-sync.svelte.ts"
  import type { Router } from "../../lib/router.svelte.ts"
  import StoreLoader from "./StoreLoader.svelte"

  let { repo, endpoint, router, documentUrl, theoryUrl, trackDocument }: {
    repo: Repo
    endpoint: string
    router: Router
    documentUrl: string
    theoryUrl: string
    trackDocument: (documentId: string) => void
  } = $props()

  interface OpenStore {
    handle: ColnHandle
    state: ColnHandleState
    sync: DocumentSync
    tables: StoreTable[]
  }

  let store = $state<OpenStore>()
  let loading = $state(false)
  let loadError = $state("")
  let selectedTableName = $state("")
  let source = $state("")
  let evaluation = $state<Evaluation>()
  let running = $state(false)
  let copyLabel = $state("Copy URL")
  let actionError = $state("")
  let operation = 0
  let copyTimer: ReturnType<typeof setTimeout> | undefined

  const document = $derived(store?.state.current)
  const selectedTable = $derived(
    store?.tables.find((table) => table.name === selectedTableName),
  )
  const rows = $derived.by<RowView[]>(() => {
    if (!document || !selectedTable) return []
    return document.scanTable(selectedTable.name)
  })
  const heads = $derived(document?.heads() ?? [])
  const syncStatus = $derived(store?.sync.status ?? "offline")
  const syncError = $derived(
    store?.sync.error instanceof Error
      ? store.sync.error.message
      : String(store?.sync.error ?? ""),
  )
  const theoryHref = $derived(
    isValidAutomergeUrl(theoryUrl) ? router.href("compiler", theoryUrl) : "",
  )

  onMount(() => {
    if (documentUrl) void loadStore(documentUrl)
  })

  onDestroy(() => {
    operation += 1
    clearTimeout(copyTimer)
  })

  function openRoute(rawUrl: string): void {
    const url = rawUrl.trim()
    loadError = ""
    if (!isValidAutomergeUrl(url)) {
      loadError = "Enter a valid automerge: document URL."
      return
    }

    router.replace("editor", url, theoryUrl)
    if (url === documentUrl) void loadStore(url)
  }

  async function loadStore(rawUrl: string): Promise<void> {
    const url = rawUrl.trim()
    const currentOperation = ++operation
    store = undefined
    evaluation = undefined
    running = false
    actionError = ""
    loadError = ""
    if (!isValidAutomergeUrl(url)) {
      loading = false
      loadError = "Enter a valid automerge: document URL."
      return
    }

    loading = true
    try {
      const handle = await find(repo, url as AutomergeUrl)
      const tables = readTables(handle.doc().jsonIR())
      if (currentOperation !== operation) return

      trackDocument(handle.documentId)
      store = {
        handle,
        state: new ColnHandleState(handle),
        sync: new DocumentSync(repo, handle),
        tables,
      }
      selectedTableName = tables[0]?.name ?? ""
      source = loadSource(handle.url)
      copyLabel = "Copy URL"
      if (import.meta.env.DEV) Object.assign(window, { repo, handle })
    } catch (cause) {
      if (currentOperation !== operation) return
      loadError = isUnavailable(cause)
        ? "That document is unavailable from the configured sync server."
        : cause instanceof Error
          ? cause.message
          : String(cause)
    } finally {
      if (currentOperation === operation) loading = false
    }
  }

  function switchStore(): void {
    operation += 1
    router.navigate("editor")
  }

  async function runProgram(): Promise<void> {
    if (!store || running) return
    const activeStore = store
    const currentOperation = operation
    saveSource(activeStore.handle.url, source)
    running = true
    evaluation = undefined
    try {
      const result = await evaluate(source, activeStore.handle)
      if (store === activeStore && currentOperation === operation) evaluation = result
    } finally {
      if (store === activeStore && currentOperation === operation) running = false
    }
  }

  async function copyUrl(): Promise<void> {
    if (!store) return
    actionError = ""
    try {
      await navigator.clipboard.writeText(store.handle.url)
      copyLabel = "Copied"
      clearTimeout(copyTimer)
      copyTimer = setTimeout(() => (copyLabel = "Copy URL"), 2_000)
    } catch (cause) {
      actionError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function isUnavailable(cause: unknown): boolean {
    return cause instanceof Error && /^Document .+ is unavailable$/.test(cause.message)
  }
</script>

{#if !store}
  <StoreLoader initialUrl={documentUrl} {loading} error={loadError} onload={openRoute} />
{:else}
  <main class="relative grid h-full min-h-0 min-w-80 grid-rows-[56px_auto] bg-[#101718] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
    {#if actionError}
      <p class="absolute top-14 left-1/2 z-30 m-0 max-w-[calc(100%-2rem)] -translate-x-1/2 border border-[#ff7657] bg-[#211918] px-4 py-2 font-['DM_Mono'] text-xs text-[#ff9a86] shadow-[0_8px_24px_#0008]" role="alert">{actionError}</p>
    {/if}

    <header class="flex min-w-0 items-center justify-between gap-3 border-b border-[#304041] px-3.5 min-[761px]:px-6">
      <button class="flex min-w-0 cursor-pointer items-center gap-3 border-0 bg-transparent p-0 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] min-[761px]:tracking-[.13em]" aria-label="Open another store" onclick={switchStore}>
        <span class="grid size-7.5 shrink-0 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
        <span class="hidden min-[470px]:inline">COLN / STORE LAB</span>
      </button>
      <div class="flex min-w-0 items-center gap-2 min-[761px]:gap-4">
        <div class="hidden min-w-0 text-right font-['DM_Mono'] min-[620px]:block">
          <div class={`flex items-center justify-end gap-2 text-[9px] tracking-[.08em] uppercase ${syncStatus === "synced" ? "text-[#d8ff57]" : syncStatus === "error" ? "text-[#ff9a86]" : "text-[#819091]"}`} data-testid="sync-status" data-status={syncStatus} title={syncStatus === "error" ? syncError : endpoint}>
            <span class={`size-1.5 rounded-full ${syncStatus === "synced" ? "bg-[#d8ff57] shadow-[0_0_10px_#d8ff5788]" : syncStatus === "error" ? "bg-[#ff7657]" : "bg-[#819091]"}`}></span>{syncStatus} / {heads.length} {heads.length === 1 ? "head" : "heads"}
          </div>
          <div class="mt-1 max-w-72 truncate text-[9px] text-[#536163]">{store.handle.url}</div>
        </div>
        {#if theoryHref}
          <a class="h-8 content-center border border-[#6b7a7b] px-2.5 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#e8ece8] uppercase no-underline hover:border-[#d8ff57] hover:text-[#d8ff57]" href={theoryHref} onclick={(event) => router.follow(event, "compiler", theoryUrl)}>Return to Theory</a>
        {/if}
        <button class="h-8 cursor-pointer border border-[#6b7a7b] bg-transparent px-2.5 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#e8ece8] uppercase hover:border-[#d8ff57] hover:text-[#d8ff57]" onclick={copyUrl} data-testid="copy-url">{copyLabel}</button>
        <button class="h-8 cursor-pointer border border-[#6b7a7b] bg-transparent px-2.5 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#e8ece8] uppercase hover:border-[#d8ff57] hover:text-[#d8ff57]" onclick={switchStore}>Switch</button>
      </div>
    </header>

    <div class="grid min-h-0 overflow-auto min-[761px]:grid-cols-[minmax(360px,1fr)_minmax(360px,1fr)] min-[761px]:overflow-hidden">
      <TableBrowser tables={store.tables} selected={selectedTable} {rows} onselect={(name) => (selectedTableName = name)} />
      <section class="flex min-h-0 flex-col bg-[#131b1c]">
        <div class="flex min-h-16 items-center justify-between gap-4 border-b border-[#304041] px-4 min-[761px]:px-5">
          <div>
            <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">JAVASCRIPT REPL</p>
            <p class="mt-1 mb-0 text-xs text-[#91a0a1]">Trusted code / handle in scope / explicit return</p>
          </div>
          <button class="h-9 cursor-pointer border border-[#d8ff57] bg-[#d8ff57] px-4 font-['DM_Mono'] text-[10px] font-medium tracking-[.12em] text-[#101718] uppercase hover:bg-transparent hover:text-[#d8ff57] disabled:cursor-wait disabled:opacity-40" disabled={running} onclick={runProgram} data-testid="run-program">{running ? "Running…" : "Run ⌘↵"}</button>
        </div>
        <ReplEditor value={source} disabled={running} onchange={(value) => (source = value)} onrun={runProgram} />
        <ReplOutput {evaluation} />
      </section>
    </div>
  </main>
{/if}
