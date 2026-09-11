<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Repo } from "@automerge/automerge-repo"
  import Home from "./Home.svelte"
  import { Router } from "../lib/router.svelte.ts"
  import { loadRecentStore } from "../tools/editor/recent-store.ts"

  let { repo, endpoint, trackDocument }: {
    repo: Repo
    endpoint: string
    trackDocument: (documentId: string) => void
  } = $props()

  const router = new Router()
  const route = $derived(router.current)
  const pageTitle = $derived(
    route.tool === "compiler"
      ? "Theory Editor — Coln Lab"
      : route.tool === "editor"
        ? "Store Editor — Coln Lab"
        : route.tool === "sync"
          ? "Graph Demo — Coln Lab"
          : route.tool === "not-found"
            ? "Page not found — Coln Lab"
            : "Coln Lab",
  )
  type NavTool = "compiler" | "editor" | "sync"
  const activeTool = $derived(
    route.tool === "compiler"
      ? { tool: "compiler" as const, label: "Theory Editor", action: "New theory", compactAction: "New" }
      : route.tool === "editor"
        ? { tool: "editor" as const, label: "Store Editor", action: "Open store", compactAction: "Open" }
        : route.tool === "sync"
          ? { tool: "sync" as const, label: "Graph Demo", action: "New graph", compactAction: "New" }
          : undefined,
  )
  const navigationGroups: Array<{
    label: string
    links: Array<{ tool: NavTool; label: string; compactLabel: string; number: string }>
  }> = [
    {
      label: "Workbench",
      links: [
        { tool: "compiler", label: "Theory Editor", compactLabel: "Theory", number: "01" },
        { tool: "editor", label: "Store Editor", compactLabel: "Store", number: "02" },
      ],
    },
    {
      label: "Demos",
      links: [
        { tool: "sync", label: "Graph Demo", compactLabel: "Graph", number: "03" },
      ],
    },
  ]
  const openDocuments = $state({
    compiler: { documentUrl: "", theoryUrl: "" },
    sync: { documentUrl: "", theoryUrl: "" },
  })
  let recentStoreUrl = $state(loadRecentStore())

  let compilerTool: ReturnType<typeof importCompilerTool> | undefined
  let editorTool: ReturnType<typeof importEditorTool> | undefined
  let graphTool: ReturnType<typeof importGraphTool> | undefined

  $effect(() => {
    if (
      route.documentUrl &&
      (route.tool === "compiler" || route.tool === "sync")
    ) {
      openDocuments[route.tool] = {
        documentUrl: route.documentUrl,
        theoryUrl: "",
      }
    }
  })

  function importCompilerTool() {
    return import("../tools/compiler/CompilerTool.svelte").then(module => module.default)
  }

  function importEditorTool() {
    return import("../tools/editor/EditorTool.svelte").then(module => module.default)
  }

  function importGraphTool() {
    return import("../tools/graph/GraphTool.svelte").then(module => module.default)
  }

  function loadCompilerTool() {
    return compilerTool ??= importCompilerTool()
  }

  function loadEditorTool() {
    return editorTool ??= importEditorTool()
  }

  function loadGraphTool() {
    return graphTool ??= importGraphTool()
  }

  function toolHref(tool: NavTool): string {
    const target = toolTarget(tool)
    return router.href(tool, target.documentUrl, target.theoryUrl)
  }

  function toolTarget(tool: NavTool) {
    if (tool === "editor") return { documentUrl: recentStoreUrl, theoryUrl: "" }
    return route.tool === tool
      ? {
          documentUrl: route.documentUrl,
          theoryUrl: "",
        }
      : openDocuments[tool]
  }

  function selectTool(
    event: MouseEvent,
    tool: NavTool,
  ): void {
    const target = toolTarget(tool)
    if (
      route.tool === tool &&
      route.documentUrl === target.documentUrl &&
      route.theoryUrl === target.theoryUrl &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.shiftKey &&
      !event.altKey
    ) {
      event.preventDefault()
      return
    }
    router.follow(event, tool, target.documentUrl, target.theoryUrl)
  }
</script>

<svelte:head><title>{pageTitle}</title></svelte:head>

<main class="grid min-h-screen min-w-80 grid-rows-[58px_1fr] bg-[var(--lab-bg)] font-['Manrope'] text-[#edf0e7] min-[761px]:h-screen min-[761px]:grid-rows-[66px_1fr]">
  <header class="relative z-30 flex min-w-0 items-center justify-between border-b border-[#36423e] bg-[#111816] px-3 min-[761px]:px-6">
    <div class="flex min-w-0 shrink-0 items-center gap-2 min-[1100px]:gap-3">
      <a class="flex shrink-0 items-center gap-3 text-[#edf0e7] no-underline" href={router.href("home")} aria-label="Coln Lab home" onclick={(event) => router.follow(event, "home")}>
        <span class="grid size-8 grid-cols-2 place-items-center border border-[#d8ff57]" aria-hidden="true"><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full border border-[#d8ff57]"></i></span>
        <span class="hidden font-['DM_Mono'] text-sm font-medium tracking-[.14em] min-[460px]:inline">COLN / LAB</span>
      </a>
      {#if activeTool}
        <span class="hidden font-['DM_Mono'] text-sm tracking-[.12em] text-[#64716b] min-[1100px]:inline" aria-hidden="true">/</span>
        <span class="hidden truncate font-['DM_Mono'] text-sm font-medium tracking-[.1em] text-[#edf0e7] uppercase min-[1100px]:inline" data-testid="active-tool-identity">{activeTool.label}</span>
        <a class="lab-secondary-action content-center no-underline" href={router.href(activeTool.tool)} aria-label={activeTool.action} data-testid="active-tool-action" onclick={(event) => router.follow(event, activeTool.tool)}><span class="min-[1100px]:hidden">{activeTool.compactAction}</span><span class="hidden min-[1100px]:inline">{activeTool.action}</span></a>
      {/if}
    </div>
    <nav class="flex h-full min-w-0 items-stretch" aria-label="Coln Lab">
      {#each navigationGroups as group, groupIndex}
        <div class={`relative flex h-full items-stretch ${groupIndex > 0 ? "ml-1 border-l-2 border-[#52605a] min-[1100px]:ml-4" : ""}`} role="group" aria-label={group.label}>
          <span class="pointer-events-none absolute top-3 left-3 z-1 hidden font-['DM_Mono'] text-xs leading-none tracking-[.12em] text-[#64716b] uppercase min-[1100px]:block" data-small-detail>{group.label}</span>
          {#each group.links as link}
            <a class={`flex min-w-16 items-center justify-center border-l border-[#29332f] px-2 font-['DM_Mono'] text-sm tracking-[.08em] uppercase no-underline min-[560px]:min-w-24 min-[560px]:px-3 min-[760px]:min-w-32 min-[1100px]:min-w-36 min-[1100px]:items-end min-[1100px]:pb-2.5 ${route.tool === link.tool ? "bg-[#d8ff57] text-[#111816]" : "text-[#8e9a94] hover:text-[#d8ff57]"}`} href={toolHref(link.tool)} aria-label={link.label} aria-current={route.tool === link.tool ? "page" : undefined} onclick={(event) => selectTool(event, link.tool)}><span><span class="mr-1 hidden text-xs opacity-55 min-[1100px]:inline" data-small-detail aria-hidden="true">{link.number}</span><span class="min-[760px]:hidden">{link.compactLabel}</span><span class="hidden min-[760px]:inline">{link.label}</span></span></a>
          {/each}
        </div>
      {/each}
    </nav>
  </header>

  <div class="min-h-0">
    {#if route.tool === "home"}
      <Home {router} />
    {:else if route.tool === "compiler"}
      {#await loadCompilerTool()}
        <div class="lab-grid-field lab-loading-label grid h-full place-items-center">OPENING THEORY EDITOR</div>
      {:then CompilerTool}
        {#key `compiler:${route.documentUrl}`}
          <CompilerTool {repo} {endpoint} {router} documentUrl={route.documentUrl} {trackDocument} />
        {/key}
      {/await}
    {:else if route.tool === "editor"}
      {#await loadEditorTool()}
        <div class="lab-grid-field lab-loading-label grid h-full place-items-center">OPENING STORE EDITOR</div>
      {:then EditorTool}
        {#key `editor:${route.documentUrl}`}
          <EditorTool {repo} {endpoint} {router} documentUrl={route.documentUrl} theoryUrl={route.theoryUrl} {trackDocument} onstoreopened={(url) => recentStoreUrl = url} />
        {/key}
      {/await}
    {:else if route.tool === "sync"}
      {#await loadGraphTool()}
        <div class="lab-grid-field lab-loading-label grid h-full place-items-center">OPENING GRAPH DEMO</div>
      {:then GraphTool}
        {#key `sync:${route.documentUrl}`}
          <GraphTool {repo} {endpoint} {router} documentUrl={route.documentUrl} {trackDocument} />
        {/key}
      {/await}
    {:else}
      <section class="lab-grid-field grid h-full place-items-center p-6 text-center"><div><p class="font-['DM_Mono'] text-xs tracking-[.18em] text-[#d8ff57]" data-small-detail>PAGE NOT FOUND</p><h1 class="text-3xl">No Coln Lab page exists at this address.</h1><a class="text-[#d8ff57]" href={router.href("home")} onclick={(event) => router.follow(event, "home")}>Return to Coln Lab</a></div></section>
    {/if}
  </div>
</main>
