<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Repo } from "@automerge/automerge-repo"
  import Home from "./Home.svelte"
  import { Router } from "../lib/router.svelte.ts"

  let { repo, endpoint, trackDocument }: {
    repo: Repo
    endpoint: string
    trackDocument: (documentId: string) => void
  } = $props()

  const router = new Router()
  const route = $derived(router.current)
  const links = [
    { tool: "compiler" as const, label: "Theory" },
    { tool: "editor" as const, label: "Store" },
    { tool: "sync" as const, label: "Graph" },
  ]
  const openDocuments = $state({
    compiler: { documentUrl: "", theoryUrl: "" },
    editor: { documentUrl: "", theoryUrl: "" },
    sync: { documentUrl: "", theoryUrl: "" },
  })

  let compilerTool: ReturnType<typeof importCompilerTool> | undefined
  let editorTool: ReturnType<typeof importEditorTool> | undefined
  let graphTool: ReturnType<typeof importGraphTool> | undefined

  $effect(() => {
    if (
      route.documentUrl &&
      (route.tool === "compiler" ||
        route.tool === "editor" ||
        route.tool === "sync")
    ) {
      openDocuments[route.tool] = {
        documentUrl: route.documentUrl,
        theoryUrl: route.tool === "editor" ? route.theoryUrl : "",
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

  function toolHref(tool: "compiler" | "editor" | "sync"): string {
    const target = toolTarget(tool)
    return router.href(tool, target.documentUrl, target.theoryUrl)
  }

  function toolTarget(tool: "compiler" | "editor" | "sync") {
    return route.tool === tool
      ? {
          documentUrl: route.documentUrl,
          theoryUrl: tool === "editor" ? route.theoryUrl : "",
        }
      : openDocuments[tool]
  }

  function selectTool(
    event: MouseEvent,
    tool: "compiler" | "editor" | "sync",
  ): void {
    if (
      route.tool === tool &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.shiftKey &&
      !event.altKey
    ) {
      event.preventDefault()
      return
    }
    const target = toolTarget(tool)
    router.follow(event, tool, target.documentUrl, target.theoryUrl)
  }
</script>

<svelte:head><title>Coln Lab</title></svelte:head>

<main class="grid min-h-screen min-w-80 grid-rows-[58px_1fr] bg-[#0d1211] font-['Manrope'] text-[#edf0e7] min-[761px]:h-screen min-[761px]:grid-rows-[66px_1fr]">
  <header class="relative z-30 flex min-w-0 items-center justify-between border-b border-[#36423e] bg-[#111816] px-3 min-[761px]:px-6">
    <a class="flex shrink-0 items-center gap-3 text-[#edf0e7] no-underline" href={router.href("home")} onclick={(event) => router.follow(event, "home")}>
      <span class="grid size-8 grid-cols-2 place-content-center gap-1 border border-[#d8ff57]" aria-hidden="true"><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full bg-[#d8ff57]"></i><i class="size-1.5 rounded-full border border-[#d8ff57]"></i></span>
      <span class="hidden font-['DM_Mono'] text-[11px] font-medium tracking-[.14em] min-[460px]:inline">COLN / LAB</span>
    </a>
    <nav class="flex h-full min-w-0 items-stretch" aria-label="Lab tools">
      {#each links as link, index}
        <a class={`grid min-w-16 place-items-center border-l border-[#29332f] px-2 font-['DM_Mono'] text-[9px] tracking-[.1em] uppercase no-underline min-[560px]:min-w-24 min-[560px]:px-4 ${route.tool === link.tool ? "bg-[#d8ff57] text-[#111816]" : "text-[#8e9a94] hover:text-[#d8ff57]"}`} href={toolHref(link.tool)} aria-current={route.tool === link.tool ? "page" : undefined} onclick={(event) => selectTool(event, link.tool)}><span><span class="mr-1 hidden text-[8px] opacity-55 min-[560px]:inline" aria-hidden="true">0{index + 1}</span>{link.label}</span></a>
      {/each}
    </nav>
  </header>

  <div class="min-h-0">
    {#if route.tool === "home"}
      <Home {router} />
    {:else if route.tool === "compiler"}
      {#await loadCompilerTool()}
        <div class="ledger-field grid h-full place-items-center font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">OPENING THEORY INSTRUMENT</div>
      {:then CompilerTool}
        {#key `compiler:${route.documentUrl}`}
          <CompilerTool {repo} {endpoint} {router} documentUrl={route.documentUrl} {trackDocument} />
        {/key}
      {/await}
    {:else if route.tool === "editor"}
      {#await loadEditorTool()}
        <div class="ledger-field grid h-full place-items-center font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">OPENING STORE INSTRUMENT</div>
      {:then EditorTool}
        {#key `editor:${route.documentUrl}`}
          <EditorTool {repo} {endpoint} {router} documentUrl={route.documentUrl} theoryUrl={route.theoryUrl} {trackDocument} />
        {/key}
      {/await}
    {:else if route.tool === "sync"}
      {#await loadGraphTool()}
        <div class="ledger-field grid h-full place-items-center font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">OPENING GRAPH INSTRUMENT</div>
      {:then GraphTool}
        {#key `sync:${route.documentUrl}`}
          <GraphTool {repo} {endpoint} {router} documentUrl={route.documentUrl} {trackDocument} />
        {/key}
      {/await}
    {:else}
      <section class="ledger-field grid h-full place-items-center p-6 text-center"><div><p class="font-['DM_Mono'] text-[10px] tracking-[.18em] text-[#d8ff57]">ENTRY NOT FOUND</p><h1 class="text-3xl">No instrument at this address.</h1><a class="text-[#d8ff57]" href={router.href("home")} onclick={(event) => router.follow(event, "home")}>Return to the ledger</a></div></section>
    {/if}
  </div>
</main>
