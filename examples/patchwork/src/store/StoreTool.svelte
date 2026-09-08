<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { isValidDocumentUrl } from "@automerge/automerge-repo"
  import { find, type ColnHandle } from "@coln-project/repo"
  import type { RowRef, RowView, Value } from "@coln-project/runtime"
  import { onDestroy, onMount, untrack } from "svelte"
  import Feedback from "../lib/Feedback.svelte"
  import SyncBadge from "../lib/SyncBadge.svelte"
  import { DocState } from "../lib/doc.svelte.ts"
  import { openDocument, type PatchworkHandle } from "../patchwork.ts"
  import PointerSetup from "../coln/PointerSetup.svelte"
  import { ColnStore } from "../coln/handle.svelte.ts"
  import type { ColnPointerDoc } from "../coln/pointer.ts"
  import { colnRepo, syncEndpoint, trackDocument } from "../coln/repo.ts"
  import { StoreSync } from "../coln/sync.svelte.ts"
  import StoreRepl from "./StoreRepl.svelte"
  import TableBrowser from "./TableBrowser.svelte"
  import { displayRowRef } from "./format.ts"
  import { readTables, type StoreTable } from "./schema.ts"
  import { THEORY_TYPE } from "../theory/datatype.ts"

  let { handle, element }: {
    handle: PatchworkHandle<ColnPointerDoc>
    element: HTMLElement
  } = $props()

  interface OpenStore {
    handle: ColnHandle
    store: ColnStore
    sync: StoreSync
    tables: StoreTable[]
  }

  const pointer = new DocState(untrack(() => handle))

  let open = $state<OpenStore>()
  let loading = $state(false)
  let loadError = $state("")
  let selectedTableName = $state("")
  let selectedRowId = $state("")
  let referenceNavigation = $state(0)
  let feedback = $state("")
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let operation = 0

  const storeUrl = $derived(pointer.current.storeUrl)
  const theoryUrl = $derived(pointer.current.theoryUrl ?? "")
  const document = $derived(open?.store.state)
  const selectedTable = $derived(
    open?.tables.find(table => table.name === selectedTableName),
  )
  const rows = $derived.by<RowView[]>(() => {
    if (!document || !selectedTable) return []
    return document.scanTable(selectedTable.name)
  })
  const heads = $derived(document?.heads() ?? [])
  const syncStatus = $derived(open?.sync.status ?? "offline")
  const syncMessage = $derived(open?.sync.message ?? "")

  onMount(() => {
    if (storeUrl) void loadStore(storeUrl)
  })

  onDestroy(() => {
    operation += 1
    clearTimeout(feedbackTimer)
  })

  /** Aim the pointer document at a store, then open it. */
  function openUrl(rawUrl: string): void {
    const url = rawUrl.trim()
    loadError = ""
    if (!isValidDocumentUrl(url, "coln")) {
      loadError = "Enter a valid Coln store url beginning with coln:."
      return
    }
    if (url !== handle.doc().storeUrl) {
      handle.change(doc => {
        doc.storeUrl = url
      })
    }
    void loadStore(url)
  }

  async function loadStore(rawUrl: string): Promise<void> {
    const url = rawUrl.trim()
    const currentOperation = ++operation
    open = undefined
    loadError = ""
    if (!isValidDocumentUrl(url, "coln")) {
      loading = false
      loadError = "Enter a valid Coln store url beginning with coln:."
      return
    }

    loading = true
    try {
      const found = await find(colnRepo(), url)
      const tables = readTables(found.doc().jsonIR())
      if (currentOperation !== operation) return

      trackDocument(found.documentId)
      open = {
        handle: found,
        store: new ColnStore(found),
        sync: new StoreSync(found),
        tables,
      }
      selectedTableName = tables[0]?.name ?? ""
      selectedRowId = ""
    } catch (cause) {
      if (currentOperation !== operation) return
      loadError = isUnavailable(cause)
        ? `That store is not available from ${syncEndpoint()}.`
        : cause instanceof Error
          ? `Could not open this store: ${cause.message}`
          : `Could not open this store: ${String(cause)}`
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

  function canFollowReference(
    tableName: string | undefined,
    value: Value,
  ): boolean {
    const reference = existingReference(value)
    return Boolean(
      document && tableName && reference && document.rowById(tableName, reference),
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

  function changeStore(): void {
    operation += 1
    open = undefined
    loadError = ""
    handle.change(doc => {
      doc.storeUrl = ""
    })
  }

  async function copyUrl(): Promise<void> {
    if (!open) return
    try {
      await navigator.clipboard.writeText(open.handle.url)
      showFeedback("Store url copied")
    } catch (cause) {
      showFeedback(cause instanceof Error ? cause.message : String(cause))
    }
  }

  function showFeedback(message: string): void {
    feedback = message
    clearTimeout(feedbackTimer)
    feedbackTimer = setTimeout(() => (feedback = ""), 2_000)
  }

  function isUnavailable(cause: unknown): boolean {
    return (
      cause instanceof Error && /^Document .+ is unavailable$/.test(cause.message)
    )
  }
</script>

<section class="coln-tool coln-store">
  {#if feedback}<Feedback message={feedback} />{/if}

  {#if !open}
    <PointerSetup
      heading="Inspect a Coln store."
      blurb="Paste a Coln store url to browse its schema, tables, rows and references."
      initialUrl={storeUrl}
      {loading}
      error={loadError}
      onopen={openUrl}
    />
  {:else}
    <!-- Read out here: the toolbar snippet is its own closure, where the
         narrowing of `open` no longer applies. -->
    {@const openStoreUrl = open.handle.url}
    <div class="panes" data-columns="2">
      <section class="pane">
        <TableBrowser
          tables={open.tables}
          selected={selectedTable}
          {rows}
          {selectedRowId}
          {referenceNavigation}
          onselect={selectTable}
          canfollow={canFollowReference}
          onfollow={followReference}
        >
          {#snippet toolbar()}
            <div class="toolbar">
              <div class="cluster toolbar-actions">
                <SyncBadge
                  status={syncStatus}
                  detail={`${heads.length} ${heads.length === 1 ? "head" : "heads"}`}
                  title={syncStatus === "error" ? syncMessage : syncEndpoint()}
                />
                {#if theoryUrl}
                  <button
                    class="action"
                    onclick={() => openDocument(element, theoryUrl, THEORY_TYPE)}
                    data-testid="open-theory"
                  >Theory</button>
                {/if}
                <button class="action" onclick={copyUrl} data-testid="copy-url">Copy url</button>
                <button class="action" onclick={changeStore} data-testid="change-store">Change store</button>
              </div>
              <code class="mono truncate store-url" title={openStoreUrl}>{openStoreUrl}</code>
            </div>
          {/snippet}
        </TableBrowser>
      </section>
      <section class="pane">
        <StoreRepl handle={open.handle} />
      </section>
    </div>
  {/if}
</section>

<style>
  .toolbar {
    display: grid;
    gap: var(--studio-space-xs, 0.375rem);
    min-width: 0;
  }

  .toolbar-actions {
    justify-content: flex-end;
  }

  .store-url {
    color: var(--coln-faint);
    display: block;
    max-width: 100%;
    text-align: right;
  }
</style>
