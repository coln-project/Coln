<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Repo } from "@automerge/automerge-repo"
  import { onDestroy } from "svelte"
  import {
    addEdge,
    addVertex,
    readGraph,
    type GraphHandle,
    type Vertex,
  } from "../lib/graph.ts"
  import { ColnHandle } from "../lib/coln-handle.svelte.ts"
  import { SubductionSync } from "../lib/subduction-sync.svelte.ts"
  import GraphCanvas from "./GraphCanvas.svelte"
  import GraphControls from "./GraphControls.svelte"

  let { handle, repo, endpoint }: {
    handle: GraphHandle
    repo: Repo
    endpoint: string
  } = $props()

  const colnHandle = $derived(new ColnHandle(handle))
  const sync = $derived(new SubductionSync(repo, handle))

  let fromId = $state("")
  let toId = $state("")
  let selectedEdgeId = $state("")
  let error = $state("")
  let feedback = $state<{ id: number; message: string }>()
  let copyPending = $state(false)
  let feedbackId = 0
  let feedbackTimeout: ReturnType<typeof setTimeout> | undefined
  let destroyed = false

  const graph = $derived(readGraph(colnHandle.state))
  const syncStatus = $derived(sync.status)
  const from = $derived(graph.vertices.find(vertex => vertex.id === fromId))
  const to = $derived(graph.vertices.find(vertex => vertex.id === toId))
  const selectedEdge = $derived(graph.edges.find(edge => edge.id === selectedEdgeId))

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
    run("Vertex added", () => addVertex(colnHandle))
  }

  function createEdge() {
    run("Edge added", () => {
      if (!from || !to) throw new Error("Choose both edge endpoints")
      addEdge(colnHandle, from, to)
      fromId = ""
      toId = ""
      selectedEdgeId = ""
    })
  }

  async function copyDocumentUrl() {
    if (copyPending) return
    copyPending = true
    error = ""
    try {
      await navigator.clipboard.writeText(handle.url)
      if (destroyed) return
      showFeedback("URL copied")
    } catch (cause) {
      if (destroyed) return
      feedback = undefined
      error = cause instanceof Error ? cause.message : String(cause)
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

  onDestroy(() => {
    destroyed = true
    clearTimeout(feedbackTimeout)
  })
</script>

<svelte:head>
  <meta name="description" content="A synchronized graph built with Coln" />
</svelte:head>

<main class="grid min-h-screen min-w-80 grid-rows-[56px_auto] bg-[#101718] font-['Manrope'] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
  {#if feedback}
    {#key feedback.id}
      <p class="fixed top-20 left-1/2 z-20 m-0 -translate-x-1/2 border border-[#d8ff57] bg-[#182122] px-4 py-2 font-['DM_Mono'] text-xs font-medium text-[#d8ff57] shadow-[0_8px_24px_#0008]" role="status" data-testid="feedback" data-feedback-id={feedback.id}>
        {feedback.message}
      </p>
    {/key}
  {/if}

  <header class="flex items-center justify-between border-b border-[#304041] px-3.5 min-[761px]:px-6">
    <a class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.08em] text-[#e8ece8] no-underline min-[761px]:tracking-[.13em]" href="/" aria-label="Create a new graph">
      <span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
      <span>COLN / GRAPH LAB</span>
    </a>
    <div class="grid min-w-0 justify-items-end gap-1 font-['DM_Mono']">
      <div class={`flex items-center gap-2 text-[11px] uppercase ${syncStatus === "synced" ? "text-[#d8ff57]" : syncStatus === "error" ? "text-[#ff9a86]" : "text-[#819091]"}`} data-testid="sync-status" data-status={syncStatus}>
        <span class={`size-1.75 rounded-full ${syncStatus === "synced" ? "bg-[#d8ff57] shadow-[0_0_12px_#d8ff5788]" : syncStatus === "error" ? "bg-[#ff7657]" : "bg-[#819091]"}`}></span>{syncStatus === "error" ? "sync error" : syncStatus}
      </div>
      <div class="flex max-w-[60vw] items-center gap-3 text-[9px] text-[#667576]">
        <span class="shrink-0" data-testid="head-count">{graph.heads.length} {graph.heads.length === 1 ? "head" : "heads"}</span>
        <code class="truncate" data-testid="subduction-url">{endpoint}</code>
      </div>
    </div>
  </header>

  <section class="flex min-h-0 flex-col min-[761px]:grid min-[761px]:grid-cols-[minmax(0,1fr)_320px]">
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
  </section>
</main>
