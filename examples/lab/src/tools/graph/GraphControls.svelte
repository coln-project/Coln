<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Edge, Graph, Vertex } from "./graph.ts"

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

<div class="flex flex-col gap-4 bg-[#182122] p-4">
  <div class="flex justify-between border-b border-[#304041] pb-3">
    <p class="m-0 font-['DM_Mono'] text-xs tracking-[.16em] text-[#748284]" data-small-detail>EDIT GRAPH</p>
  </div>

  <button class="lab-primary-action flex h-[52px] items-center justify-between border-0 px-4 font-bold" onclick={onaddvertex} data-testid="add-vertex">
    <span>Add vertex</span><b class="font-['DM_Mono'] text-2xl">+</b>
  </button>

  <div class="grid gap-2.5">
    <div class="grid gap-[7px]">
      <label class="font-['DM_Mono'] text-sm tracking-[.12em] text-[#91a0a1] uppercase" for="from">Source vertex</label>
      <select class="h-11 w-full rounded-none border border-[#304041] bg-[#101718] px-2.5 text-[#e8ece8]" id="from" value={from?.id ?? ""} onchange={event => onfromchange(event.currentTarget.value)} data-testid="from-select">
        <option value="">Choose a source vertex</option>
        {#each graph.vertices as vertex}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </div>
    <div class="pl-3 font-['DM_Mono'] text-lg text-[#6d7b7d]">↓</div>
    <div class="grid gap-[7px]">
      <label class="font-['DM_Mono'] text-sm tracking-[.12em] text-[#91a0a1] uppercase" for="to">Target vertex</label>
      <select class="h-11 w-full rounded-none border border-[#304041] bg-[#101718] px-2.5 text-[#e8ece8]" id="to" value={to?.id ?? ""} onchange={event => ontochange(event.currentTarget.value)} data-testid="to-select">
        <option value="">Choose a target vertex</option>
        {#each graph.vertices as vertex}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </div>
    <button class={`${from && to ? "lab-primary-action" : "lab-outlined-action"} mt-1 flex h-[46px] w-full items-center justify-between px-[13px] font-bold disabled:cursor-not-allowed disabled:opacity-35`} disabled={!from || !to} onclick={onaddedge} data-testid="add-edge">
      Add directed edge <span class="text-xl text-[#d8ff57]">→</span>
    </button>
  </div>

  {#if selectedEdge}
    <div class="grid gap-1.5 border-l-3 border-[#d8ff57] bg-[#101718] p-3.5" data-testid="selected-edge-details">
      <small class="font-['DM_Mono'] text-xs tracking-[.12em] text-[#839193]" data-small-detail>SELECTED EDGE</small>
      <strong class="font-['DM_Mono'] text-base font-medium">{vertexLabel(selectedEdge.fromId)} → {vertexLabel(selectedEdge.toId)}</strong>
      <code class="font-['DM_Mono'] text-sm text-[#697879]">{shortId(selectedEdge.id)}</code>
    </div>
  {:else if from}
    <div class="grid gap-1.5 border-l-3 border-[#d8ff57] bg-[#101718] p-3.5">
      <small class="font-['DM_Mono'] text-xs tracking-[.12em] text-[#839193]" data-small-detail>{to ? "EDGE READY TO ADD" : "SOURCE SELECTED"}</small>
      <strong class="font-['DM_Mono'] text-base font-medium">{from.label}{to ? ` → ${to.label}` : ""}</strong>
      <code class="font-['DM_Mono'] text-sm text-[#697879]">{shortId(from.id)}</code>
    </div>
  {/if}

  {#if error}<p class="m-0 font-['DM_Mono'] text-sm leading-normal text-[#ff9a86]" role="alert">{error}</p>{/if}

</div>
