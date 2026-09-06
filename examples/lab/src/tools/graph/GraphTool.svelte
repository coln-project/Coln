<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import {
    isValidAutomergeUrl,
    type AutomergeUrl,
    type Repo,
  } from "@automerge/automerge-repo"
  import { create, find } from "@coln-project/repo"
  import { onDestroy, onMount } from "svelte"
  import GraphCanvas from "./GraphCanvas.svelte"
  import GraphControls from "./GraphControls.svelte"
  import * as GraphRealm from "./generated/GraphRealm.ts"
  import { ColnHandle } from "./coln-handle.svelte.ts"
  import {
    addEdge,
    addVertex,
    readGraph,
    type Graph,
    type GraphHandle,
    type Vertex,
  } from "./graph.ts"
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

  const emptyGraph: Graph = { vertices: [], edges: [], heads: [] }

  let handle = $state<GraphHandle>()
  let loadError = $state<LoadError>()
  let fromId = $state("")
  let toId = $state("")
  let selectedEdgeId = $state("")
  let error = $state("")
  let feedback = $state<{ id: number; message: string }>()
  let copyPending = $state(false)
  let feedbackId = 0
  let feedbackTimeout: ReturnType<typeof setTimeout> | undefined
  let destroyed = false

  const colnHandle = $derived(handle ? new ColnHandle(handle) : undefined)
  const sync = $derived(handle ? new DocumentSync(repo, handle) : undefined)
  const graph = $derived(colnHandle ? readGraph(colnHandle.state) : emptyGraph)
  const syncStatus = $derived(sync?.status ?? "offline")
  const from = $derived(graph.vertices.find(vertex => vertex.id === fromId))
  const to = $derived(graph.vertices.find(vertex => vertex.id === toId))
  const selectedEdge = $derived(graph.edges.find(edge => edge.id === selectedEdgeId))

  onMount(() => {
    if (documentUrl && !isValidAutomergeUrl(documentUrl)) {
      loadError = "invalid"
      return
    }

    void loadDocument()
  })

  onDestroy(() => {
    destroyed = true
    clearTimeout(feedbackTimeout)
  })

  async function loadDocument() {
    try {
      const loaded = documentUrl
        ? await find(repo, documentUrl as AutomergeUrl, GraphRealm)
        : create(repo, GraphRealm)
      if (destroyed) return

      if (!documentUrl) {
        router.replace("sync", loaded.url)
        return
      }
      handle = loaded
      trackDocument(loaded.documentId)
    } catch (cause) {
      if (!destroyed) loadError = classifyLoadError(cause)
    }
  }

  function classifyLoadError(cause: unknown): LoadError {
    if (
      cause instanceof TypeError &&
      (/schema does not match/i.test(cause.message) || /different realm bindings/i.test(cause.message))
    ) return "incompatible"
    return "unavailable"
  }

  function selectVertex(vertex: Vertex) {
    selectedEdgeId = ""
    if (!from || to) {
      fromId = vertex.id
      toId = ""
    } else {
      toId = vertex.id
    }
  }

  function createVertex() {
    if (colnHandle) run("Vertex added", () => addVertex(colnHandle))
  }

  function createEdge() {
    if (!colnHandle) return
    run("Edge added", () => {
      if (!from || !to) throw new Error("Choose both edge endpoints")
      addEdge(colnHandle, from, to)
      fromId = ""
      toId = ""
      selectedEdgeId = ""
    })
  }

  function createNewGraph() {
    const next = create(repo, GraphRealm)
    router.navigate("sync", next.url)
  }

  async function copyDocumentUrl() {
    if (!handle || copyPending) return
    copyPending = true
    error = ""
    try {
      await navigator.clipboard.writeText(handle.url)
      if (!destroyed) showFeedback("URL copied")
    } catch (cause) {
      if (!destroyed) {
        feedback = undefined
        error = cause instanceof Error ? cause.message : String(cause)
      }
    } finally {
      if (!destroyed) copyPending = false
    }
  }

  function run(message: string, action: () => void) {
    error = ""
    try {
      action()
      showFeedback(message)
    } catch (cause) {
      feedback = undefined
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function showFeedback(message: string) {
    const id = ++feedbackId
    feedback = { id, message }
    clearTimeout(feedbackTimeout)
    feedbackTimeout = setTimeout(() => {
      if (feedback?.id === id) feedback = undefined
    }, 2_000)
  }

  const errorCopy = {
    invalid: {
      label: "DOCUMENT URL INVALID",
      heading: "This is not a valid Automerge document URL.",
      detail: "Check the URL and try again, or create a new graph.",
    },
    unavailable: {
      label: "DOCUMENT UNAVAILABLE",
      heading: "This graph could not be found.",
      detail: "It may not have been synchronized yet, or the URL may point to a document that no longer exists.",
    },
    incompatible: {
      label: "DOCUMENT INCOMPATIBLE",
      heading: "This document is not a supported Coln graph.",
      detail: "The document uses a schema incompatible with GraphRealm.",
    },
  } satisfies Record<LoadError, { label: string; heading: string; detail: string }>
</script>

{#if loadError}
  <section class="grid h-full min-h-[520px] place-items-center bg-[#101718] p-6" data-testid="document-load-error" data-error-kind={loadError}>
    <div class="grid w-full max-w-xl gap-6 border border-[#304041] bg-[#182122] p-6 min-[601px]:p-10">
      <div class="grid gap-3 border-l-3 border-[#ff7657] pl-5">
        <p class="m-0 font-['DM_Mono'] text-xs tracking-[.14em] text-[#ff9a86]">{errorCopy[loadError].label}</p>
        <h1 class="m-0 text-3xl font-semibold tracking-[-.03em]">{errorCopy[loadError].heading}</h1>
        <p class="m-0 leading-7 text-[#aab6b6]">{errorCopy[loadError].detail}</p>
      </div>
      {#if documentUrl}
        <code class="overflow-hidden text-ellipsis border border-[#304041] bg-[#101718] p-3 font-['DM_Mono'] text-xs text-[#839193]" data-testid="failed-document-url">{documentUrl}</code>
      {/if}
      <button class="flex h-12 cursor-pointer items-center justify-between border-0 bg-[#d8ff57] px-4 font-bold text-[#101718]" type="button" onclick={createNewGraph}>
        Create a new graph <span class="font-['DM_Mono'] text-xl">+</span>
      </button>
    </div>
  </section>
{:else if handle}
  {@const currentGraphUrl = handle.url}
  <section class="relative grid h-full min-h-0 grid-rows-[56px_auto] bg-[#101718] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
    {#if feedback}
      {#key feedback.id}
        <p class="absolute top-16 left-1/2 z-20 m-0 -translate-x-1/2 border border-[#d8ff57] bg-[#182122] px-4 py-2 font-['DM_Mono'] text-xs font-medium text-[#d8ff57] shadow-[0_8px_24px_#0008]" role="status">
          {feedback.message}
        </p>
      {/key}
    {/if}

    <header class="flex min-w-0 items-center justify-between gap-3 border-b border-[#304041] px-3.5 min-[761px]:px-6">
      <button class="flex shrink-0 cursor-pointer items-center gap-3 border-0 bg-transparent p-0 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] min-[761px]:tracking-[.13em]" type="button" aria-label="Create a new graph" onclick={createNewGraph}>
        <span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
        <span class="hidden min-[470px]:inline">COLN / GRAPH LAB</span>
      </button>
      <div class="flex min-w-0 items-center gap-3">
        <a class="shrink-0 font-['DM_Mono'] text-[9px] tracking-[.08em] text-[#d8ff57] uppercase no-underline" href={router.href("editor", currentGraphUrl)} onclick={(event) => router.follow(event, "editor", currentGraphUrl)}>Open in Store</a>
        <div class="grid min-w-0 justify-items-end gap-1 font-['DM_Mono']">
          <div class={`flex items-center gap-2 text-[11px] uppercase ${syncStatus === "synced" ? "text-[#d8ff57]" : syncStatus === "error" ? "text-[#ff9a86]" : "text-[#819091]"}`} data-testid="sync-status" data-status={syncStatus}>
            <span class={`size-1.75 rounded-full ${syncStatus === "synced" ? "bg-[#d8ff57] shadow-[0_0_12px_#d8ff5788]" : syncStatus === "error" ? "bg-[#ff7657]" : "bg-[#819091]"}`}></span>{syncStatus === "error" ? "sync error" : syncStatus}
          </div>
          <div class="flex max-w-[40vw] items-center gap-3 text-[9px] text-[#667576]">
            <span class="shrink-0" data-testid="head-count">{graph.heads.length} {graph.heads.length === 1 ? "head" : "heads"}</span>
            <code class="hidden truncate min-[760px]:block" data-testid="subduction-url">{endpoint}</code>
          </div>
        </div>
      </div>
    </header>

    <div class="flex min-h-0 flex-col min-[761px]:grid min-[761px]:grid-cols-[minmax(0,1fr)_320px]">
      <GraphCanvas
        {graph}
        {from}
        {to}
        {selectedEdgeId}
        documentUrl={handle.url}
        {copyPending}
        oncopydocumenturl={copyDocumentUrl}
        onaddvertex={createVertex}
        onselectedge={id => selectedEdgeId = id}
        onselectvertex={selectVertex}
      />
      <GraphControls
        {graph}
        {from}
        {to}
        {selectedEdge}
        {error}
        onaddvertex={createVertex}
        onaddedge={createEdge}
        onfromchange={id => fromId = id}
        ontochange={id => toId = id}
      />
    </div>
  </section>
{:else}
  <section class="grid h-full min-h-[520px] place-items-center bg-[#101718]" aria-live="polite">
    <p class="font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">LOADING GRAPH</p>
  </section>
{/if}
