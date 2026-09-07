<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import {
    isValidDocumentUrl,
    type Repo,
  } from "@automerge/automerge-repo"
  import { create, find, type ColnUrl } from "@coln-project/repo"
  import { Pane, PaneGroup } from "paneforge"
  import { onDestroy, onMount } from "svelte"
  import LabPaneResizer from "../../lib/LabPaneResizer.svelte"
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
  import DocumentLoadError from "../../app/components/DocumentLoadError.svelte"
  import FeedbackNotice from "../../app/components/FeedbackNotice.svelte"
  import SyncStatus from "../../app/components/SyncStatus.svelte"
  import StoreRepl from "../editor/StoreRepl.svelte"
  import StoreSummary from "../editor/StoreSummary.svelte"
  import { readTables } from "../editor/schema.ts"

  type LoadError = "invalid" | "unavailable" | "incompatible"
  type GraphPanel = "graph" | "repl"

  let { repo, endpoint, router, documentUrl, trackDocument }: {
    repo: Repo
    endpoint: string
    router: Router
    documentUrl: string
    trackDocument: (documentId: string) => void
  } = $props()

  const emptyGraph: Graph = { vertices: [], edges: [], heads: [] }
  const storeTables = readTables(JSON.stringify(GraphRealm.schema))
  const panels: Array<{ id: GraphPanel; label: string }> = [
    { id: "graph", label: "Graph" },
    { id: "repl", label: "Store REPL" },
  ]

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
  let activePanel = $state<GraphPanel>("graph")

  const colnHandle = $derived(handle ? new ColnHandle(handle) : undefined)
  const sync = $derived(handle ? new DocumentSync(repo, handle) : undefined)
  const graph = $derived(colnHandle ? readGraph(colnHandle.state) : emptyGraph)
  const syncStatus = $derived(sync?.status ?? "offline")
  const from = $derived(graph.vertices.find(vertex => vertex.id === fromId))
  const to = $derived(graph.vertices.find(vertex => vertex.id === toId))
  const selectedEdge = $derived(graph.edges.find(edge => edge.id === selectedEdgeId))
  const tableSummaries = $derived(
    storeTables.map(table => ({
      ...table,
      rowCount: colnHandle?.state.scanTable(table.name).length ?? 0,
    })),
  )

  onMount(() => {
    if (documentUrl && !isValidDocumentUrl(documentUrl, "coln")) {
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
        ? await find(repo, documentUrl as ColnUrl, GraphRealm)
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
      if (!from || !to) throw new Error("Choose both a source and a target vertex.")
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
      if (!destroyed) showFeedback("Graph document URL copied")
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

  function selectPanel(panel: GraphPanel, focus = false) {
    activePanel = panel
    if (focus) requestAnimationFrame(() => document.getElementById(`graph-${panel}-tab`)?.focus())
  }

  function handleTabKey(event: KeyboardEvent, index: number) {
    let nextIndex: number | undefined
    if (event.key === "ArrowRight") nextIndex = (index + 1) % panels.length
    else if (event.key === "ArrowLeft") nextIndex = (index - 1 + panels.length) % panels.length
    else if (event.key === "Home") nextIndex = 0
    else if (event.key === "End") nextIndex = panels.length - 1
    if (nextIndex === undefined) return
    event.preventDefault()
    selectPanel(panels[nextIndex].id, true)
  }

  const errorCopy = {
    invalid: {
      label: "INVALID GRAPH URL",
      heading: "This is not a valid Graph Demo document URL.",
      detail: "Check the URL and try again, or start a new Graph Demo document.",
    },
    unavailable: {
      label: "GRAPH UNAVAILABLE",
      heading: "This graph could not be found.",
      detail: "The sync server may be unreachable, or the document may no longer exist.",
    },
    incompatible: {
      label: "INCOMPATIBLE GRAPH",
      heading: "This document is not a supported Coln graph.",
      detail: "The document does not use the schema required by Graph Demo.",
    },
  } satisfies Record<LoadError, { label: string; heading: string; detail: string }>
</script>

{#if loadError}
  <DocumentLoadError label={errorCopy[loadError].label} heading={errorCopy[loadError].heading} detail={errorCopy[loadError].detail} {documentUrl} errorKind={loadError} action="Start a new Graph Demo" onaction={createNewGraph} />
{:else if handle}
  <section class="lab-tool relative flex h-full min-h-0 flex-col">
    {#if feedback}
      {#key feedback.id}
        <FeedbackNotice message={feedback.message} />
      {/key}
    {/if}

    <PaneGroup class="lab-desktop-pane-group min-h-0 flex-1" direction="horizontal" autoSaveId="coln-lab-graph-workspace">
      <Pane id="graph-canvas-pane" class="min-h-0" defaultSize={67} minSize={45} maxSize={80}>
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
      </Pane>
      <LabPaneResizer label="Resize graph canvas and sidebar" orientation="vertical" testId="graph-workspace-resizer" />
      <Pane id="graph-sidebar-pane" class="min-h-0" defaultSize={33} minSize={20} maxSize={55}>
      <aside class="flex h-full min-h-0 flex-col bg-[#182122]" data-testid="graph-sidebar">
        <div class="flex min-h-10 shrink-0 items-center justify-between gap-3 border-b border-[#304041] px-4 font-['DM_Mono']">
          <SyncStatus status={syncStatus} compact title={syncStatus === "error" ? String(sync?.error ?? "") : `Sync server: ${endpoint}`} />
          <span class="shrink-0 text-sm text-[#667576]" data-testid="head-count">{graph.heads.length} {graph.heads.length === 1 ? "head" : "heads"}</span>
        </div>
        <div class="grid h-12 shrink-0 grid-cols-2 border-b border-[#304041]" role="tablist" aria-label="Graph Demo workspace">
          {#each panels as panel, index}
            <button class={`cursor-pointer border-0 border-r border-[#304041] font-['DM_Mono'] text-sm tracking-[.08em] uppercase last:border-r-0 ${activePanel === panel.id ? "bg-[#d8ff57] text-[#101718]" : "bg-[#131b1c] text-[#91a0a1] hover:text-[#d8ff57]"}`} id={`graph-${panel.id}-tab`} type="button" role="tab" aria-selected={activePanel === panel.id} aria-controls={`graph-${panel.id}-panel`} tabindex={activePanel === panel.id ? 0 : -1} onclick={() => selectPanel(panel.id)} onkeydown={(event) => handleTabKey(event, index)}>{panel.label}</button>
          {/each}
        </div>

        <div class="min-h-0 flex-1 flex-col overflow-auto" class:flex={activePanel === "graph"} id="graph-graph-panel" role="tabpanel" aria-labelledby="graph-graph-tab" hidden={activePanel !== "graph"}>
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
          <div class="mt-auto shrink-0">
            <StoreSummary tables={tableSummaries} headCount={graph.heads.length} />
          </div>
        </div>

        <div class="min-h-0 flex-1 flex-col" class:flex={activePanel === "repl"} id="graph-repl-panel" role="tabpanel" aria-labelledby="graph-repl-tab" hidden={activePanel !== "repl"}>
          <StoreRepl {handle} active={activePanel === "repl"} compact layoutId="coln-lab-graph-repl" />
        </div>
      </aside>
      </Pane>
    </PaneGroup>
  </section>
{:else}
  <section class="lab-loading min-h-[520px]" aria-live="polite">
    <p>LOADING GRAPH DEMO</p>
  </section>
{/if}
