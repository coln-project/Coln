<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Edge, Graph, Vertex } from "../lib/graph.ts"

  type Point = Vertex & { x: number; y: number }
  type DrawableEdge = Edge & { path: string }

  let {
    graph,
    from,
    to,
    selectedEdgeId,
    documentUrl,
    copyPending,
    oncopydocumenturl,
    onaddvertex,
    onselectedge,
    onselectvertex,
  }: {
    graph: Graph
    from?: Vertex
    to?: Vertex
    selectedEdgeId: string
    documentUrl: string
    copyPending: boolean
    oncopydocumenturl: () => void | Promise<void>
    onaddvertex: () => void
    onselectedge: (id: string) => void
    onselectvertex: (vertex: Vertex) => void
  } = $props()

  const points = $derived(layoutVertices(graph.vertices))
  const pointById = $derived(new Map(points.map(point => [point.id, point])))
  const drawableEdges = $derived(layoutEdges(graph.edges, pointById))
  const previewEdgePath = $derived.by(() => {
    if (!from || !to) return undefined
    const source = pointById.get(from.id)
    const target = pointById.get(to.id)
    if (!source || !target) return undefined

    const siblingIndex = graph.edges.filter(edge =>
      edge.fromId === from.id && edge.toId === to.id
    ).length
    const reverseCount = from.id === to.id ? 0 : graph.edges.filter(edge =>
      edge.fromId === to.id && edge.toId === from.id
    ).length
    const offset = reverseCount > 0
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
      const angle = (index / Math.max(vertices.length, 1)) * Math.PI * 2 - Math.PI / 2
      return {
        ...vertex,
        x: 500 + Math.cos(angle) * radius,
        y: 330 + Math.sin(angle) * radius,
      }
    })
  }

  function layoutEdges(edges: Edge[], vertices: Map<string, Point>): DrawableEdge[] {
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
      const siblingIndex = siblings.findIndex(candidate => candidate.id === edge.id)
      const reverseCount = edge.fromId === edge.toId
        ? 0
        : groups.get(`${edge.toId}\0${edge.fromId}`)?.length ?? 0
      const offset = reverseCount > 0
        ? (siblingIndex + 0.5) * 28
        : (siblingIndex - (siblings.length - 1) / 2) * 28

      return [{ ...edge, path: edgePath(source, target, offset, siblingIndex) }]
    })
  }

  function edgePath(source: Point, target: Point, offset: number, index: number) {
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
</script>

<svelte:window onclick={() => onselectedge("")} />

<div class="relative min-h-[500px] overflow-hidden border-b border-[#304041] min-[761px]:min-h-[680px] min-[761px]:border-r min-[761px]:border-b-0">
  <div class="absolute top-0 right-0 left-2 z-2 grid min-w-0 max-w-[480px] gap-3 bg-[#101718e6] p-4">
    <small class="font-['DM_Mono'] text-xs tracking-[.12em] text-[#aab6b6]">DOCUMENT URL</small>
    <div class="flex min-w-0">
      <input class="min-w-0 flex-1 border border-r-0 border-[#405152] bg-[#101718] p-3 font-['DM_Mono'] text-xs text-[#d2dada]" readonly value={documentUrl} onfocus={(event) => event.currentTarget.select()} data-testid="doc-url" />
      <button class="shrink-0 cursor-pointer border border-[#405152] bg-[#182122] px-4 font-['DM_Mono'] text-xs font-medium text-[#d8ff57] hover:bg-[#243031] disabled:cursor-wait disabled:opacity-50" type="button" aria-label="Copy document URL" disabled={copyPending} onclick={oncopydocumenturl} data-testid="copy-document-url">{copyPending ? "copying" : "copy"}</button>
    </div>
  </div>

  {#if graph.vertices.length === 0}
    <button class="absolute top-1/2 left-1/2 z-3 grid w-[220px] -translate-1/2 cursor-pointer justify-items-center gap-2 border border-dashed border-[#536263] bg-[#101718d9] px-5 py-[30px] text-[#e8ece8]" onclick={onaddvertex} data-testid="empty-add-vertex">
      <span class="font-['DM_Mono'] text-[42px] font-light text-[#d8ff57]">+</span>
      <strong class="text-sm">Place the first vertex</strong>
      <small class="font-['DM_Mono'] text-[11px] text-[#748284]">The graph is empty</small>
    </button>
  {/if}

  <svg class="block min-h-[500px] w-full min-[761px]:min-h-[680px] min-[761px]:h-full" viewBox="0 0 1000 660" role="img" aria-label="Directed graph" data-testid="graph-canvas">
    <defs>
      <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
        <path class="fill-[#738486]" d="M 0 0 L 10 5 L 0 10 z"></path>
      </marker>
      <marker id="preview-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
        <path class="fill-[#d8ff57]" d="M 0 0 L 10 5 L 0 10 z"></path>
      </marker>
      <pattern id="grid" width="32" height="32" patternUnits="userSpaceOnUse">
        <circle class="fill-[#354344]" cx="1" cy="1" r="1"></circle>
      </pattern>
    </defs>
    <rect width="1000" height="660" fill="url(#grid)"></rect>

    {#each drawableEdges as edge (edge.id)}
      <path
        class={`cursor-pointer fill-none outline-none transition-[stroke,opacity] duration-150 [pointer-events:stroke] hover:stroke-[#d8ff57] hover:opacity-100 hover:stroke-3 focus-visible:stroke-[#e8ece8] focus-visible:stroke-4 ${edge.id === selectedEdgeId ? "stroke-[#65d9e7] opacity-100 stroke-3" : "stroke-[#738486] opacity-70 stroke-2"}`}
        d={edge.path}
        marker-end="url(#arrow)"
        onclick={(event) => {
          event.stopPropagation()
          onselectedge(edge.id)
        }}
        onkeydown={(event) => event.key === "Enter" && onselectedge(edge.id)}
        role="button"
        tabindex="0"
        aria-label={`Edge from ${vertexLabel(edge.fromId)} to ${vertexLabel(edge.toId)}`}
        data-testid="graph-edge"
      ></path>
    {/each}

    {#if previewEdgePath}
      <path
        class="pointer-events-none fill-none stroke-[#d8ff57] stroke-3 opacity-75 [stroke-dasharray:2_9] [stroke-linecap:round]"
        d={previewEdgePath}
        marker-end="url(#preview-arrow)"
        data-testid="edge-preview"
      ></path>
    {/if}

    {#each points as vertex (vertex.id)}
      <g
        class="group cursor-pointer outline-none"
        transform={`translate(${vertex.x} ${vertex.y})`}
        onclick={() => onselectvertex(vertex)}
        onkeydown={(event) => event.key === "Enter" && onselectvertex(vertex)}
        role="button"
        tabindex="0"
        data-testid="graph-vertex"
      >
        <title>{vertex.id}</title>
        <circle
          class={`transition-[fill,stroke,stroke-width] duration-150 group-hover:stroke-[#e8ece8] group-hover:stroke-3 group-focus:stroke-[#e8ece8] group-focus:stroke-3 ${vertex.id === to?.id ? "fill-[#17383d] stroke-[#65d9e7] stroke-4" : vertex.id === from?.id ? "fill-[#43301d] stroke-[#ff7657] stroke-4" : "fill-[#172021] stroke-[#839193] stroke-2"}`}
          r="38"
        ></circle>
        <text class="pointer-events-none fill-[#e8ece8] font-['DM_Mono'] text-base font-semibold" text-anchor="middle" dominant-baseline="central">{vertex.label}</text>
      </g>
    {/each}
  </svg>

  <div class="absolute bottom-5 left-6 flex gap-4 font-['DM_Mono'] text-[16px] text-[#829092]">
    <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-[#ff7657]"></i>source</span>
    <span class="flex items-center gap-1.5"><i class="size-2 rounded-full bg-[#65d9e7]"></i>target</span>
  </div>
</div>
