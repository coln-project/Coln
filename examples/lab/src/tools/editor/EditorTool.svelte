<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import {
    isValidAutomergeUrl,
    isValidDocumentUrl,
    type Repo,
  } from "@automerge/automerge-repo"
  import { find, type ColnHandle } from "@coln-project/repo"
  import type { RowRef, RowView, Value } from "@coln-project/runtime"
  import { Pane, PaneGroup } from "paneforge"
  import { onDestroy, onMount } from "svelte"
  import LabPaneResizer from "../../lib/LabPaneResizer.svelte"
  import StoreRepl from "./StoreRepl.svelte"
  import TableBrowser from "./TableBrowser.svelte"
  import { ColnHandleState } from "./coln-handle.svelte.ts"
  import {
    readTables,
    type StoreTable,
  } from "./schema.ts"
  import { DocumentSync } from "../../lib/document-sync.svelte.ts"
  import type { Router } from "../../lib/router.svelte.ts"
  import StoreLoader from "./StoreLoader.svelte"
  import FeedbackNotice from "../../app/components/FeedbackNotice.svelte"
  import SyncStatus from "../../app/components/SyncStatus.svelte"
  import { loadRecentStore, saveRecentStore } from "./recent-store.ts"
  import { displayRowRef } from "./format.ts"

  let { repo, endpoint, router, documentUrl, theoryUrl, trackDocument, onstoreopened }: {
    repo: Repo
    endpoint: string
    router: Router
    documentUrl: string
    theoryUrl: string
    trackDocument: (documentId: string) => void
    onstoreopened: (url: string) => void
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
  let selectedRowId = $state("")
  let referenceNavigation = $state(0)
  let copyLabel = $state("Copy store URL")
  let actionError = $state("")
  let operation = 0
  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let recentStoreUrl = $state(loadRecentStore())

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
  const compactCopyLabel = $derived(
    copyLabel === "Store URL copied" ? "Copied" : "Copy",
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
    if (!isValidDocumentUrl(url, "coln")) {
      loadError = "Enter a valid Coln store URL beginning with coln:."
      return
    }

    router.replace("editor", url)
    if (url === documentUrl) void loadStore(url)
  }

  async function loadStore(rawUrl: string): Promise<void> {
    const url = rawUrl.trim()
    const currentOperation = ++operation
    store = undefined
    actionError = ""
    loadError = ""
    if (!isValidDocumentUrl(url, "coln")) {
      loading = false
      loadError = "Enter a valid Coln store URL beginning with coln:."
      return
    }

    loading = true
    try {
      const handle = await find(repo, url)
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
      selectedRowId = ""
      recentStoreUrl = handle.url
      saveRecentStore(handle.url)
      onstoreopened(handle.url)
      copyLabel = "Copy store URL"
      if (import.meta.env.DEV) Object.assign(window, { repo, handle })
    } catch (cause) {
      if (currentOperation !== operation) return
      loadError = isUnavailable(cause)
        ? "That store is unavailable from the configured sync server."
        : cause instanceof Error
          ? `Store Editor could not open this store: ${cause.message}`
          : `Store Editor could not open this store: ${String(cause)}`
    } finally {
      if (currentOperation === operation) loading = false
    }
  }

  function selectTable(name: string): void {
    selectedTableName = name
    selectedRowId = ""
  }

  function existingReference(value: Value): RowRef | undefined {
    if (value.tag !== "row_id" || !("existing" in value.value)) return undefined
    return value.value
  }

  function canFollowReference(tableName: string | undefined, value: Value): boolean {
    const reference = existingReference(value)
    return Boolean(
      document &&
        tableName &&
        reference &&
        document.rowById(tableName, reference),
    )
  }

  function followReference(tableName: string | undefined, value: Value): void {
    const reference = existingReference(value)
    if (
      !document ||
      !tableName ||
      !reference ||
      !document.rowById(tableName, reference)
    ) return

    selectedRowId = displayRowRef(reference).full
    selectedTableName = tableName
    referenceNavigation += 1
  }

  async function copyUrl(): Promise<void> {
    if (!store) return
    actionError = ""
    try {
      await navigator.clipboard.writeText(store.handle.url)
      copyLabel = "Store URL copied"
      clearTimeout(copyTimer)
      copyTimer = setTimeout(() => (copyLabel = "Copy store URL"), 2_000)
    } catch (cause) {
      actionError = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function isUnavailable(cause: unknown): boolean {
    return cause instanceof Error && /^Document .+ is unavailable$/.test(cause.message)
  }
</script>

{#if !store}
  <StoreLoader initialUrl={documentUrl} recentUrl={recentStoreUrl} {loading} error={loadError} onload={openRoute} />
{:else}
  {@const storeUrl = store.handle.url}
  <section class="lab-tool relative flex h-full min-h-0 min-w-80 flex-col">
    {#if actionError}
      <FeedbackNotice message={actionError} kind="error" />
    {/if}

    <PaneGroup class="lab-desktop-pane-group min-h-0 flex-1" direction="horizontal" autoSaveId="coln-lab-store-workspace">
      <Pane id="store-browser-pane" class="min-h-0" defaultSize={50} minSize={30} maxSize={70}>
        <TableBrowser
          tables={store.tables}
          selected={selectedTable}
          {rows}
          {selectedRowId}
          {referenceNavigation}
          onselect={selectTable}
          canfollow={canFollowReference}
          onfollow={followReference}
        >
          {#snippet toolbar()}
            <div class="grid min-w-0 gap-2 font-['DM_Mono']">
              <div class="flex flex-wrap items-center justify-end gap-2">
                <SyncStatus status={syncStatus} compact detail={`${heads.length} ${heads.length === 1 ? "head" : "heads"}`} title={syncStatus === "error" ? syncError : endpoint} />
                {#if theoryHref}
                  <a class="lab-secondary-action content-center no-underline" href={theoryHref} aria-label="Return to Theory Editor" onclick={(event) => router.follow(event, "compiler", theoryUrl)}><span class="min-[1050px]:hidden">Theory</span><span class="hidden min-[1050px]:inline">Return to Theory Editor</span></a>
                {/if}
                <button class="lab-secondary-action" aria-label={copyLabel} onclick={copyUrl} data-testid="copy-url"><span class="min-[1050px]:hidden">{compactCopyLabel}</span><span class="hidden min-[1050px]:inline">{copyLabel}</span></button>
              </div>
              <code class="block max-w-full truncate text-right text-sm text-[#536163]" title={storeUrl}>{storeUrl}</code>
            </div>
          {/snippet}
        </TableBrowser>
      </Pane>
      <LabPaneResizer label="Resize store browser and JavaScript REPL" orientation="vertical" testId="store-workspace-resizer" />
      <Pane id="store-repl-pane" class="min-h-0" defaultSize={50} minSize={30} maxSize={70}>
        <StoreRepl handle={store.handle} layoutId="coln-lab-store-repl" />
      </Pane>
    </PaneGroup>
  </section>
{/if}
