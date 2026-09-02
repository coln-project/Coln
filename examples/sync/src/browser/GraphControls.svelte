<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Edge, Graph, Vertex } from "../lib/graph.ts"

  let {
    graph,
    from,
    to,
    selectedEdge,
    error,
    onaddvertex,
    onaddedge,
    onfromchange,
    ontochange,
  }: {
    graph: Graph
    from?: Vertex
    to?: Vertex
    selectedEdge?: Edge
    error: string
    onaddvertex: () => void
    onaddedge: () => void
    onfromchange: (id: string) => void
    ontochange: (id: string) => void
  } = $props()

  function vertexLabel(id: string) {
    return graph.vertices.find(vertex => vertex.id === id)?.label ?? "?"
  }

  function shortId(id: string) {
    return id.slice(0, 8)
  }
</script>

<aside class="flex flex-col gap-[22px] bg-[#182122] p-[18px] min-[761px]:p-6">
  <div class="flex justify-between border-b border-[#304041] pb-3">
    <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">BUILD</p>
    <span class="font-['DM_Mono'] text-[10px] text-[#748284]">01</span>
  </div>

  <button class="flex h-[52px] cursor-pointer items-center justify-between border-0 bg-[#d8ff57] px-4 font-bold text-[#101718]" onclick={onaddvertex} data-testid="add-vertex">
    <span>Add vertex</span><b class="font-['DM_Mono'] text-2xl">+</b>
  </button>

  <div class="grid gap-2.5">
    <div class="grid gap-[7px]">
      <label class="font-['DM_Mono'] text-[10px] tracking-[.12em] text-[#91a0a1] uppercase" for="from">Source</label>
      <select class="h-11 w-full rounded-none border border-[#304041] bg-[#101718] px-2.5 text-[#e8ece8]" id="from" value={from?.id ?? ""} onchange={event => onfromchange(event.currentTarget.value)} data-testid="from-select">
        <option value="">Choose vertex</option>
        {#each graph.vertices as vertex}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </div>
    <div class="pl-3 font-['DM_Mono'] text-lg text-[#6d7b7d]">↓</div>
    <div class="grid gap-[7px]">
      <label class="font-['DM_Mono'] text-[10px] tracking-[.12em] text-[#91a0a1] uppercase" for="to">Target</label>
      <select class="h-11 w-full rounded-none border border-[#304041] bg-[#101718] px-2.5 text-[#e8ece8]" id="to" value={to?.id ?? ""} onchange={event => ontochange(event.currentTarget.value)} data-testid="to-select">
        <option value="">Choose vertex</option>
        {#each graph.vertices as vertex}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </div>
    <button class="mt-1 flex h-[46px] cursor-pointer items-center justify-between border border-[#6b7a7b] bg-transparent px-[13px] font-bold text-[#e8ece8] disabled:cursor-not-allowed disabled:opacity-35" disabled={!from || !to} onclick={onaddedge} data-testid="add-edge">
      Draw directed edge <span class="text-xl text-[#d8ff57]">→</span>
    </button>
  </div>

  {#if selectedEdge}
    <div class="grid gap-1.5 border-l-3 border-[#d8ff57] bg-[#101718] p-3.5" data-testid="selected-edge-details">
      <small class="font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#839193]">SELECTED EDGE</small>
      <strong class="font-['DM_Mono'] text-base font-medium">{vertexLabel(selectedEdge.fromId)} → {vertexLabel(selectedEdge.toId)}</strong>
      <code class="font-['DM_Mono'] text-[10px] text-[#697879]">{shortId(selectedEdge.id)}</code>
    </div>
  {:else if from}
    <div class="grid gap-1.5 border-l-3 border-[#d8ff57] bg-[#101718] p-3.5">
      <small class="font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#839193]">{to ? "EDGE READY" : "SOURCE SELECTED"}</small>
      <strong class="font-['DM_Mono'] text-base font-medium">{from.label}{to ? ` → ${to.label}` : ""}</strong>
      <code class="font-['DM_Mono'] text-[10px] text-[#697879]">{shortId(from.id)}</code>
    </div>
  {/if}

  {#if error}<p class="m-0 font-['DM_Mono'] text-[11px] leading-normal text-[#ff9a86]" role="alert">{error}</p>{/if}

  <div class="mt-auto grid min-w-0 gap-2 border-t border-[#304041] pt-[18px]">
    <small class="font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#839193]">LIVE DOCUMENT</small>
    <strong class="font-['DM_Mono'] text-sm font-medium text-[#d8ff57]" data-testid="graph-counts">
      {graph.vertices.length} {graph.vertices.length === 1 ? "VERTEX" : "VERTICES"} / {graph.edges.length} {graph.edges.length === 1 ? "EDGE" : "EDGES"}
    </strong>
  </div>
</aside>
