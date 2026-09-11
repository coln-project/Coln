<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Router } from "../lib/router.svelte.ts"
  let { router }: { router: Router } = $props()

  const workbenchTools = [
    { tool: "compiler" as const, number: "01", name: "Theory Editor", action: "Open Theory Editor", description: "Define a theory, compile its realms, and create stores from them." },
    { tool: "editor" as const, number: "02", name: "Store Editor", action: "Open Store Editor", description: "Inspect a store’s tables, rows, and references, or query them with JavaScript." },
  ]
  const graphDemo = { tool: "sync" as const, number: "03", name: "Graph Demo", action: "Open Graph Demo", description: "Explore a graph represented by tables in a Coln store, then query or change it through the embedded JavaScript REPL." }
</script>

<section class="lab-grid-field h-full min-h-0 overflow-auto px-4 py-10 min-[761px]:px-8 min-[761px]:py-16">
  <div class="mx-auto max-w-6xl">
    <div class="mb-10 grid gap-6 border-b border-[var(--lab-border)] pb-10 min-[761px]:grid-cols-[1fr_auto] min-[761px]:items-start">
      <div>
        <p class="m-0 font-['DM_Mono'] text-xs tracking-[.2em] text-[#a7ba69] uppercase" data-small-detail>Coln Lab</p>
        <h1 class="mt-4 mb-3 max-w-3xl text-4xl leading-[1.05] font-semibold tracking-[-.04em] text-[#f0f1e8] min-[761px]:text-6xl">Design theories. Inspect stores. Explore <span class="text-[var(--lab-accent)]">Coln</span>.</h1>
        <p class="m-0 max-w-2xl text-sm leading-7 text-[#9ba6a0] min-[761px]:text-base">Coln is a data-oriented proof assistant that treats proofs as structured, inspectable data.</p>
      </div>
      <div class="grid grid-cols-5 gap-2" aria-hidden="true">
        {#each Array(10) as _, index}
          <span class={`size-3 rounded-full border ${index < 7 ? "border-[#d8ff57] bg-[#d8ff57]" : "border-[#52605a] bg-[#17201d]"}`}></span>
        {/each}
      </div>
    </div>

    <section aria-labelledby="workbench-heading">
      <div class="mb-3 flex items-center justify-between font-['DM_Mono'] text-xs tracking-[.16em] uppercase" data-small-detail>
        <h2 class="m-0 text-[#a7ba69]" id="workbench-heading">Workbench</h2>
        <span class="text-[#64716b]">Define / Inspect</span>
      </div>
      <div class="grid gap-px border border-[var(--lab-border)] bg-[var(--lab-border)] min-[760px]:grid-cols-2">
      {#each workbenchTools as item}
        <a class="group grid min-h-64 grid-rows-[auto_1fr_auto] bg-[#111816] p-6 text-[#edf0e7] no-underline transition-colors hover:bg-[#18221e]" href={router.href(item.tool)} onclick={(event) => router.follow(event, item.tool)}>
          <div class="flex items-center justify-between font-['DM_Mono'] text-xs tracking-[.16em] uppercase" data-small-detail>
            <span class="text-[#d8ff57]">{item.number}</span>
            <span class="text-[#64716b]">Workbench / {item.name}</span>
          </div>
          <div class="self-center py-8">
            <h2 class="m-0 text-3xl font-semibold tracking-[-.03em]">{item.name}</h2>
            <p class="mt-3 mb-0 max-w-xs text-sm leading-6 text-[#929e98]">{item.description}</p>
          </div>
          <div class="flex items-center justify-between border-t border-[#303c37] pt-4 font-['DM_Mono'] text-sm tracking-[.1em] uppercase group-hover:text-[#d8ff57]">
            <span>{item.action}</span><span>→</span>
          </div>
        </a>
      {/each}
      </div>
    </section>

    <section class="mt-10" aria-labelledby="demos-heading">
      <div class="mb-3 flex items-center justify-between font-['DM_Mono'] text-xs tracking-[.16em] uppercase" data-small-detail>
        <h2 class="m-0 text-[#a7ba69]" id="demos-heading">Demos</h2>
        <span class="text-[#64716b]">Explore</span>
      </div>
      <a class="group grid min-h-64 grid-rows-[auto_1fr_auto] border border-[var(--lab-border)] bg-[#111816] p-6 text-[#edf0e7] no-underline transition-colors hover:bg-[#18221e]" href={router.href(graphDemo.tool)} onclick={(event) => router.follow(event, graphDemo.tool)}>
        <div class="flex items-center justify-between font-['DM_Mono'] text-xs tracking-[.16em] uppercase" data-small-detail>
          <span class="text-[#d8ff57]">{graphDemo.number}</span>
          <span class="text-[#64716b]">Demos / {graphDemo.name}</span>
        </div>
        <div class="self-center py-8">
          <h2 class="m-0 text-3xl font-semibold tracking-[-.03em]">{graphDemo.name}</h2>
          <p class="mt-3 mb-0 max-w-xl text-sm leading-6 text-[#929e98]">{graphDemo.description}</p>
        </div>
        <div class="flex items-center justify-between border-t border-[#303c37] pt-4 font-['DM_Mono'] text-sm tracking-[.1em] uppercase group-hover:text-[#d8ff57]">
          <span>{graphDemo.action}</span><span>→</span>
        </div>
      </a>
    </section>
  </div>
</section>
