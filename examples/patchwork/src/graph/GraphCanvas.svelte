<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Edge, Graph, Vertex } from "./graph.ts"

  type Point = Vertex & { x: number; y: number }
  type DrawableEdge = Edge & { path: string }

  let {
    graph,
    from,
    to,
    selectedEdgeId,
    onaddvertex,
    onselectedge,
    onselectvertex,
  }: {
    graph: Graph
    from?: Vertex
    to?: Vertex
    selectedEdgeId: string
    onaddvertex: () => void
    onselectedge: (id: string) => void
    onselectvertex: (vertex: Vertex) => void
  } = $props()

  let canvas: HTMLDivElement

  const points = $derived(layoutVertices(graph.vertices))
  const pointById = $derived(new Map(points.map(point => [point.id, point])))
  const drawableEdges = $derived(layoutEdges(graph.edges, pointById))
  const previewEdgePath = $derived.by(() => {
    if (!from || !to) return undefined
    const source = pointById.get(from.id)
    const target = pointById.get(to.id)
    if (!source || !target) return undefined

    const siblingIndex = graph.edges.filter(
      edge => edge.fromId === from.id && edge.toId === to.id,
    ).length
    const reverseCount =
      from.id === to.id
        ? 0
        : graph.edges.filter(
            edge => edge.fromId === to.id && edge.toId === from.id,
          ).length
    const offset =
      reverseCount > 0
        ? siblingIndex > 0
          ? (siblingIndex + 0.5) * 28
          : ((reverseCount + 1) / 2) * 28
        : siblingIndex * 14
    return edgePath(source, target, offset, siblingIndex)
  })

  function vertexLabel(id: string) {
    return graph.vertices.find(vertex => vertex.id === id)?.label ?? "?"
  }

  function layoutVertices(vertices: Vertex[]): Point[] {
    if (vertices.length === 1) return [{ ...vertices[0], x: 500, y: 330 }]
    const radius = Math.min(245, 120 + vertices.length * 18)
    return vertices.map((vertex, index) => {
      const angle =
        (index / Math.max(vertices.length, 1)) * Math.PI * 2 - Math.PI / 2
      return {
        ...vertex,
        x: 500 + Math.cos(angle) * radius,
        y: 330 + Math.sin(angle) * radius,
      }
    })
  }

  function layoutEdges(
    edges: Edge[],
    vertices: Map<string, Point>,
  ): DrawableEdge[] {
    const groups = new Map<string, Edge[]>()
    for (const edge of edges) {
      const key = `${edge.fromId}\0${edge.toId}`
      groups.set(key, [...(groups.get(key) ?? []), edge])
    }

    return edges.flatMap(edge => {
      const source = vertices.get(edge.fromId)
      const target = vertices.get(edge.toId)
      if (!source || !target) return []
      const siblings = groups.get(`${edge.fromId}\0${edge.toId}`) ?? [edge]
      const siblingIndex = siblings.findIndex(
        candidate => candidate.id === edge.id,
      )
      const reverseCount =
        edge.fromId === edge.toId
          ? 0
          : (groups.get(`${edge.toId}\0${edge.fromId}`)?.length ?? 0)
      const offset =
        reverseCount > 0
          ? (siblingIndex + 0.5) * 28
          : (siblingIndex - (siblings.length - 1) / 2) * 28

      return [{ ...edge, path: edgePath(source, target, offset, siblingIndex) }]
    })
  }

  function edgePath(
    source: Point,
    target: Point,
    offset: number,
    index: number,
  ) {
    if (source.id === target.id) {
      const spread = 58 + index * 16
      const direction = source.y < 170 ? 1 : -1
      const anchorY = source.y + direction * 28
      const controlY = source.y + direction * (110 + spread / 3)
      return `M ${source.x - 18} ${anchorY} C ${source.x - spread} ${controlY}, ${source.x + spread} ${controlY}, ${source.x + 18} ${anchorY}`
    }

    const dx = target.x - source.x
    const dy = target.y - source.y
    const length = Math.hypot(dx, dy)
    const ux = dx / length
    const uy = dy / length
    const startX = source.x + ux * 40
    const startY = source.y + uy * 40
    const endX = target.x - ux * 46
    const endY = target.y - uy * 46
    const middleX = (startX + endX) / 2 - uy * offset
    const middleY = (startY + endY) / 2 + ux * offset
    return `M ${startX} ${startY} Q ${middleX} ${middleY}, ${endX} ${endY}`
  }

  /**
   * Clicking the background clears the edge selection. Deliberately not
   * `stopPropagation()` on the edge itself: Patchwork's host frameworks
   * delegate click at the document, and stopping it there breaks them.
   */
  function clearSelectedEdge(event: MouseEvent) {
    const target = event.target
    if (!(target instanceof Node) || !canvas.contains(target)) return
    if (target instanceof Element && target.closest("[data-edge]")) return
    onselectedge("")
  }
</script>

<svelte:window onclick={clearSelectedEdge} />

<div class="canvas" bind:this={canvas}>
  {#if graph.vertices.length === 0}
    <button class="first-vertex" onclick={onaddvertex} data-testid="empty-add-vertex">
      <span class="plus" aria-hidden="true">+</span>
      <strong>Add the first vertex</strong>
      <small class="mono">This graph has no vertices.</small>
    </button>
  {/if}

  <svg
    viewBox="0 0 1000 660"
    role="img"
    aria-label="Coln graph"
    data-testid="graph-canvas"
  >
    <defs>
      <marker id="coln-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
        <path class="arrow-head" d="M 0 0 L 10 5 L 0 10 z"></path>
      </marker>
      <marker id="coln-preview-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
        <path class="arrow-head-preview" d="M 0 0 L 10 5 L 0 10 z"></path>
      </marker>
      <pattern id="coln-grid" width="32" height="32" patternUnits="userSpaceOnUse">
        <circle class="grid-dot" cx="1" cy="1" r="1"></circle>
      </pattern>
    </defs>
    <rect width="1000" height="660" fill="url(#coln-grid)"></rect>

    {#each drawableEdges as edge (edge.id)}
      <path
        class="edge"
        data-edge
        data-selected={edge.id === selectedEdgeId || undefined}
        d={edge.path}
        marker-end="url(#coln-arrow)"
        onclick={() => onselectedge(edge.id)}
        onkeydown={event => event.key === "Enter" && onselectedge(edge.id)}
        role="button"
        tabindex="0"
        aria-label={`Directed edge from ${vertexLabel(edge.fromId)} to ${vertexLabel(edge.toId)}`}
        data-testid="graph-edge"
      ></path>
    {/each}

    {#if previewEdgePath}
      <path class="preview" d={previewEdgePath} marker-end="url(#coln-preview-arrow)" data-testid="edge-preview"></path>
    {/if}

    {#each points as vertex (vertex.id)}
      <g
        class="vertex"
        transform={`translate(${vertex.x} ${vertex.y})`}
        onclick={() => onselectvertex(vertex)}
        onkeydown={event => event.key === "Enter" && onselectvertex(vertex)}
        role="button"
        tabindex="0"
        data-role={vertex.id === to?.id ? "target" : vertex.id === from?.id ? "source" : undefined}
        data-testid="graph-vertex"
      >
        <title>{vertex.label}: {vertex.id}</title>
        <circle r="38"></circle>
        <text text-anchor="middle" dominant-baseline="central">{vertex.label}</text>
      </g>
    {/each}
  </svg>

  <div class="legend mono">
    <span data-role="source">edge source</span>
    <span data-role="target">edge target</span>
  </div>
</div>

<style>
  .canvas {
    flex: 1;
    min-height: 20rem;
    overflow: hidden;
    position: relative;
  }

  svg {
    display: block;
    height: 100%;
    min-height: 20rem;
    width: 100%;
  }

  .grid-dot {
    fill: var(--coln-border-strong);
  }

  .arrow-head {
    fill: var(--coln-muted);
  }

  .arrow-head-preview {
    fill: var(--coln-accent);
  }

  .edge {
    cursor: pointer;
    fill: none;
    opacity: 0.7;
    outline: none;
    pointer-events: stroke;
    stroke: var(--coln-muted);
    stroke-width: 2;
    transition: stroke var(--studio-transition-fast, 0.1s ease),
      opacity var(--studio-transition-fast, 0.1s ease);
  }

  .edge:hover,
  .edge:focus-visible {
    opacity: 1;
    stroke: var(--coln-accent);
    stroke-width: 3;
  }

  .edge[data-selected] {
    opacity: 1;
    stroke: var(--coln-link-text);
    stroke-width: 3;
  }

  .preview {
    fill: none;
    opacity: 0.75;
    pointer-events: none;
    stroke: var(--coln-accent);
    stroke-dasharray: 2 9;
    stroke-linecap: round;
    stroke-width: 3;
  }

  .vertex {
    cursor: pointer;
    outline: none;
  }

  .vertex circle {
    fill: var(--coln-surface-raised);
    stroke: var(--coln-border-strong);
    stroke-width: 2;
    transition: fill var(--studio-transition-fast, 0.1s ease),
      stroke var(--studio-transition-fast, 0.1s ease);
  }

  .vertex:hover circle,
  .vertex:focus-visible circle {
    stroke: var(--coln-line);
    stroke-width: 3;
  }

  .vertex[data-role="source"] circle {
    fill: color-mix(in oklch, var(--coln-danger), var(--coln-fill) 80%);
    stroke: var(--coln-danger);
    stroke-width: 4;
  }

  .vertex[data-role="target"] circle {
    fill: color-mix(in oklch, var(--coln-link-text), var(--coln-fill) 80%);
    stroke: var(--coln-link-text);
    stroke-width: 4;
  }

  .vertex text {
    fill: var(--coln-line);
    font-family: var(--coln-mono);
    font-size: 30px;
    font-weight: 600;
    pointer-events: none;
  }

  .first-vertex {
    background: var(--coln-fill);
    border: 1px dashed var(--coln-border-strong);
    border-radius: var(--studio-radius-md, 8px);
    color: var(--coln-line);
    cursor: pointer;
    display: grid;
    gap: 0.5rem;
    justify-items: center;
    left: 50%;
    padding: 1.75rem 1.25rem;
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 14rem;
    z-index: 2;
  }

  .plus {
    color: var(--coln-accent-text);
    font-family: var(--coln-mono);
    font-size: 2.5rem;
    line-height: 1;
  }

  .first-vertex small {
    color: var(--coln-faint);
  }

  .legend {
    bottom: 1rem;
    color: var(--coln-muted);
    display: flex;
    gap: var(--studio-space-md, 1rem);
    left: 1.25rem;
    position: absolute;
  }

  .legend span {
    align-items: center;
    display: flex;
    gap: 0.375rem;
  }

  .legend span::before {
    border-radius: var(--studio-radius-round, 9999px);
    content: "";
    height: 0.5rem;
    width: 0.5rem;
  }

  .legend span[data-role="source"]::before {
    background: var(--coln-danger);
  }

  .legend span[data-role="target"]::before {
    background: var(--coln-link-text);
  }
</style>
