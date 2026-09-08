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

<div class="controls">
  <p class="label">Edit graph</p>

  <button class="action wide" data-primary onclick={onaddvertex} data-testid="add-vertex">
    <span>Add vertex</span><span aria-hidden="true">+</span>
  </button>

  <div class="edge">
    <label class="choice" for="coln-graph-from">
      <span class="label">Source vertex</span>
      <select
        class="field"
        id="coln-graph-from"
        value={from?.id ?? ""}
        onchange={event => onfromchange(event.currentTarget.value)}
        data-testid="from-select"
      >
        <option value="">Choose a source vertex</option>
        {#each graph.vertices as vertex (vertex.id)}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </label>
    <span class="arrow" aria-hidden="true">↓</span>
    <label class="choice" for="coln-graph-to">
      <span class="label">Target vertex</span>
      <select
        class="field"
        id="coln-graph-to"
        value={to?.id ?? ""}
        onchange={event => ontochange(event.currentTarget.value)}
        data-testid="to-select"
      >
        <option value="">Choose a target vertex</option>
        {#each graph.vertices as vertex (vertex.id)}
          <option value={vertex.id}>{vertex.label}</option>
        {/each}
      </select>
    </label>
    <button
      class="action wide"
      data-primary={from && to ? true : undefined}
      disabled={!from || !to}
      onclick={onaddedge}
      data-testid="add-edge"
    >
      <span>Add directed edge</span><span aria-hidden="true">→</span>
    </button>
  </div>

  {#if selectedEdge}
    <div class="detail" data-testid="selected-edge-details">
      <span class="label">Selected edge</span>
      <strong class="mono">
        {vertexLabel(selectedEdge.fromId)} → {vertexLabel(selectedEdge.toId)}
      </strong>
      <code class="mono faint">{shortId(selectedEdge.id)}</code>
    </div>
  {:else if from}
    <div class="detail">
      <span class="label">{to ? "Edge ready to add" : "Source selected"}</span>
      <strong class="mono">{from.label}{to ? ` → ${to.label}` : ""}</strong>
      <code class="mono faint">{shortId(from.id)}</code>
    </div>
  {/if}

  {#if error}<p class="alert" role="alert">{error}</p>{/if}
</div>

<style>
  .controls {
    display: grid;
    gap: var(--studio-space-md, 1rem);
    padding: var(--studio-space-md, 1rem);
  }

  .wide {
    align-items: center;
    display: flex;
    justify-content: space-between;
    padding: 0.75rem 0.875rem;
    width: 100%;
  }

  .edge {
    display: grid;
    gap: var(--studio-space-xs, 0.375rem);
  }

  .choice {
    display: grid;
    gap: 0.375rem;
  }

  .arrow {
    color: var(--coln-faint);
    font-family: var(--coln-mono);
    padding-left: var(--studio-space-sm, 0.75rem);
  }

  .detail {
    background: var(--coln-fill);
    border-left: 3px solid var(--coln-accent);
    display: grid;
    gap: 0.375rem;
    padding: var(--studio-space-sm, 0.75rem);
  }

  .faint {
    color: var(--coln-faint);
  }
</style>
