<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { isValidDocumentUrl } from "@automerge/automerge-repo"
  import { create, find, type ColnUrl } from "@coln-project/repo"
  import { onDestroy, onMount, untrack } from "svelte"
  import Feedback from "../lib/Feedback.svelte"
  import SyncBadge from "../lib/SyncBadge.svelte"
  import { DocState } from "../lib/doc.svelte.ts"
  import PointerSetup from "../coln/PointerSetup.svelte"
  import { ColnStore } from "../coln/handle.svelte.ts"
  import type { ColnPointerDoc } from "../coln/pointer.ts"
  import { colnRepo, syncEndpoint, trackDocument } from "../coln/repo.ts"
  import { StoreSync } from "../coln/sync.svelte.ts"
  import type { PatchworkHandle } from "../patchwork.ts"
  import StoreRepl from "../store/StoreRepl.svelte"
  import { readTables } from "../store/schema.ts"
  import GraphCanvas from "./GraphCanvas.svelte"
  import GraphControls from "./GraphControls.svelte"
  import StoreSummary from "./StoreSummary.svelte"
  import * as GraphRealm from "./generated/GraphRealm.ts"
  import {
    addEdge,
    addVertex,
    readGraph,
    type Graph,
    type GraphHandle,
    type Vertex,
  } from "./graph.ts"

  let { handle }: { handle: PatchworkHandle<ColnPointerDoc> } = $props()

  type Panel = "graph" | "repl"

  const emptyGraph: Graph = { vertices: [], edges: [], heads: [] }
  const storeTables = readTables(JSON.stringify(GraphRealm.schema))
  const panels: Array<{ id: Panel; label: string }> = [
    { id: "graph", label: "Graph" },
    { id: "repl", label: "Store REPL" },
  ]

  const pointer = new DocState(untrack(() => handle))

  let graphHandle = $state<GraphHandle>()
  let store = $state<ColnStore<typeof GraphRealm>>()
  let sync = $state<StoreSync>()
  let loading = $state(false)
  let loadError = $state("")
  let fromId = $state("")
  let toId = $state("")
  let selectedEdgeId = $state("")
  let error = $state("")
  let feedback = $state("")
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let activePanel = $state<Panel>("graph")
  let operation = 0
  let destroyed = false

  const storeUrl = $derived(pointer.current.storeUrl)
  const graph = $derived(store ? readGraph(store.state) : emptyGraph)
  const from = $derived(graph.vertices.find(vertex => vertex.id === fromId))
  const to = $derived(graph.vertices.find(vertex => vertex.id === toId))
  const selectedEdge = $derived(
    graph.edges.find(edge => edge.id === selectedEdgeId),
  )
  const syncStatus = $derived(sync?.status ?? "offline")
  const tableSummaries = $derived(
    storeTables.map(table => ({
      ...table,
      rowCount: store?.state.scanTable(table.name).length ?? 0,
    })),
  )

  onMount(() => {
    if (storeUrl) void loadStore(storeUrl)
  })

  onDestroy(() => {
    destroyed = true
    operation += 1
    clearTimeout(feedbackTimer)
  })

  function openUrl(rawUrl: string): void {
    const url = rawUrl.trim()
    loadError = ""
    if (!isValidDocumentUrl(url, "coln")) {
      loadError = "Enter a valid Coln store url beginning with coln:."
      return
    }
    aim(url)
    void loadStore(url)
  }

  async function loadStore(rawUrl: string): Promise<void> {
    const url = rawUrl.trim()
    const currentOperation = ++operation
    graphHandle = undefined
    store = undefined
    sync = undefined
    loadError = ""
    loading = true
    try {
      const found = await find(colnRepo(), url as ColnUrl, GraphRealm)
      if (destroyed || currentOperation !== operation) return
      adopt(found)
    } catch (cause) {
      if (destroyed || currentOperation !== operation) return
      loadError = describeLoadError(cause)
    } finally {
      if (!destroyed && currentOperation === operation) loading = false
    }
  }

  /**
   * Create a store for the checked-in GraphRealm schema, flush it so a
   * collaborator following the pointer finds it, and aim this document at it.
   */
  async function createGraphStore(): Promise<void> {
    const currentOperation = ++operation
    loadError = ""
    loading = true
    try {
      const repo = colnRepo()
      const created = create(repo, GraphRealm)
      await repo.flush([created.documentId])
      if (destroyed || currentOperation !== operation) return
      aim(created.url)
      adopt(created)
    } catch (cause) {
      if (!destroyed && currentOperation === operation) {
        loadError = describeLoadError(cause)
      }
    } finally {
      if (!destroyed && currentOperation === operation) loading = false
    }
  }

  function adopt(found: GraphHandle): void {
    trackDocument(found.documentId)
    graphHandle = found
    store = new ColnStore(found)
    sync = new StoreSync(found)
    fromId = ""
    toId = ""
    selectedEdgeId = ""
  }

  function aim(url: string): void {
    if (url === handle.doc().storeUrl) return
    handle.change(doc => {
      doc.storeUrl = url
      doc.realmName = "GraphRealm"
    })
  }

  function describeLoadError(cause: unknown): string {
    if (
      cause instanceof TypeError &&
      (/schema does not match/i.test(cause.message) ||
        /different realm bindings/i.test(cause.message))
    ) {
      return "That store does not use the schema the Graph Demo needs."
    }
    if (cause instanceof Error && /is unavailable$/.test(cause.message)) {
      return `That store is not available from ${syncEndpoint()}.`
    }
    return cause instanceof Error ? cause.message : String(cause)
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
    if (store) run("Vertex added", () => addVertex(store!))
  }

  function createEdge() {
    if (!store) return
    run("Edge added", () => {
      if (!from || !to) {
        throw new Error("Choose both a source and a target vertex.")
      }
      addEdge(store!, from, to)
      fromId = ""
      toId = ""
      selectedEdgeId = ""
    })
  }

  function changeStore(): void {
    operation += 1
    graphHandle = undefined
    store = undefined
    sync = undefined
    loadError = ""
    handle.change(doc => {
      doc.storeUrl = ""
    })
  }

  async function copyUrl(): Promise<void> {
    if (!graphHandle) return
    try {
      await navigator.clipboard.writeText(graphHandle.url)
      showFeedback("Store url copied")
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function run(message: string, action: () => void) {
    error = ""
    try {
      action()
      showFeedback(message)
    } catch (cause) {
      feedback = ""
      error = cause instanceof Error ? cause.message : String(cause)
    }
  }

  function showFeedback(message: string) {
    feedback = message
    clearTimeout(feedbackTimer)
    feedbackTimer = setTimeout(() => (feedback = ""), 2_000)
  }

  function handleTabKey(event: KeyboardEvent, index: number) {
    let nextIndex: number | undefined
    if (event.key === "ArrowRight") nextIndex = (index + 1) % panels.length
    else if (event.key === "ArrowLeft") {
      nextIndex = (index - 1 + panels.length) % panels.length
    } else if (event.key === "Home") nextIndex = 0
    else if (event.key === "End") nextIndex = panels.length - 1
    if (nextIndex === undefined) return
    event.preventDefault()
    activePanel = panels[nextIndex].id
    const id = `coln-graph-${activePanel}-tab`
    requestAnimationFrame(() => document.getElementById(id)?.focus())
  }
</script>

<section class="coln-tool coln-graph">
  {#if feedback}<Feedback message={feedback} />{/if}

  {#if !store || !graphHandle}
    <PointerSetup
      heading="Open or create a graph store."
      blurb="The Graph Demo edits a Coln store built from the GraphRealm schema — vertices and directed edges."
      initialUrl={storeUrl}
      {loading}
      error={loadError}
      createLabel="Create a new graph"
      onopen={openUrl}
      oncreate={() => void createGraphStore()}
    />
  {:else}
    <div class="panes" data-columns="2">
      <section class="pane canvas-pane">
        <GraphCanvas
          {graph}
          {from}
          {to}
          {selectedEdgeId}
          onaddvertex={createVertex}
          onselectedge={id => (selectedEdgeId = id)}
          onselectvertex={selectVertex}
        />
      </section>
      <aside class="pane sidebar" data-testid="graph-sidebar">
        <div class="pane-head">
          <SyncBadge
            status={syncStatus}
            detail={`${graph.heads.length} ${graph.heads.length === 1 ? "head" : "heads"}`}
            title={syncStatus === "error" ? (sync?.message ?? "") : syncEndpoint()}
          />
          <div class="cluster">
            <button class="action" onclick={copyUrl} data-testid="copy-url">Copy url</button>
            <button class="action" onclick={changeStore} data-testid="change-store">Change store</button>
          </div>
        </div>

        <div class="tabs" role="tablist" aria-label="Graph workspace">
          {#each panels as panel, index (panel.id)}
            <button
              class="tab"
              id={`coln-graph-${panel.id}-tab`}
              type="button"
              role="tab"
              aria-selected={activePanel === panel.id}
              aria-controls={`coln-graph-${panel.id}-panel`}
              data-selected={activePanel === panel.id || undefined}
              tabindex={activePanel === panel.id ? 0 : -1}
              onclick={() => (activePanel = panel.id)}
              onkeydown={event => handleTabKey(event, index)}
            >{panel.label}</button>
          {/each}
        </div>

        <div
          class="panel"
          id="coln-graph-graph-panel"
          role="tabpanel"
          aria-labelledby="coln-graph-graph-tab"
          hidden={activePanel !== "graph"}
        >
          <GraphControls
            {graph}
            {from}
            {to}
            {selectedEdge}
            {error}
            onaddvertex={createVertex}
            onaddedge={createEdge}
            onfromchange={id => (fromId = id)}
            ontochange={id => (toId = id)}
          />
          <div class="summary">
            <StoreSummary tables={tableSummaries} headCount={graph.heads.length} />
          </div>
        </div>

        <div
          class="panel repl-panel"
          id="coln-graph-repl-panel"
          role="tabpanel"
          aria-labelledby="coln-graph-repl-tab"
          hidden={activePanel !== "repl"}
        >
          <StoreRepl handle={graphHandle} compact />
        </div>
      </aside>
    </div>
  {/if}
</section>

<style>
  .canvas-pane {
    min-height: 20rem;
  }

  .sidebar {
    background: var(--coln-surface-raised);
    display: grid;
    grid-template-rows: auto auto minmax(0, 1fr);
    min-height: 0;
  }

  .tabs {
    border-bottom: 1px solid var(--coln-border);
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .tab {
    background: var(--coln-fill);
    border: 0;
    border-right: 1px solid var(--coln-border);
    color: var(--coln-muted);
    cursor: pointer;
    font-family: var(--coln-mono);
    font-size: 0.8125rem;
    letter-spacing: 0.08em;
    padding: 0.75rem 0.5rem;
    text-transform: uppercase;
  }

  .tab:last-child {
    border-right: 0;
  }

  .tab:hover {
    color: var(--coln-accent-text);
  }

  .tab[data-selected] {
    background: var(--coln-accent);
    color: var(--coln-accent-line);
  }

  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }

  .panel[hidden] {
    display: none;
  }

  .repl-panel {
    overflow: hidden;
  }

  .summary {
    margin-top: auto;
  }
</style>
