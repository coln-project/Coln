<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Router } from "../lib/router.svelte.ts"
  let { router }: { router: Router } = $props()

  const tools = [
    { tool: "compiler" as const, number: "01", name: "Theory", action: "Open a theory", description: "Define formal systems, compile realms, and create empty instances." },
    { tool: "editor" as const, number: "02", name: "Store", action: "Inspect a store", description: "Read table ledgers and run trusted JavaScript against a live handle." },
    { tool: "sync" as const, number: "03", name: "Graph", action: "Start an experiment", description: "Count vertices and relations in a synchronized graph." },
  ]
</script>

<section class="ledger-field min-h-0 overflow-auto px-4 py-10 min-[761px]:px-8 min-[761px]:py-16">
  <div class="mx-auto max-w-6xl">
    <div class="mb-10 grid gap-6 border-b border-[#36423e] pb-10 min-[761px]:grid-cols-[1fr_auto] min-[761px]:items-end">
      <div>
        <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.2em] text-[#a7ba69] uppercase">Ledger 000 / Counting room</p>
        <h1 class="mt-4 mb-3 max-w-3xl text-4xl leading-[1.05] font-semibold tracking-[-.04em] text-[#f0f1e8] min-[761px]:text-6xl">A workbench for things that follow.</h1>
        <p class="m-0 max-w-2xl text-sm leading-7 text-[#9ba6a0] min-[761px]:text-base">Coln takes its name from the pebble: a small, durable mark for counting. Use the ledger below to define theories, inspect stores, and test relations.</p>
      </div>
      <div class="grid grid-cols-5 gap-2" aria-hidden="true">
        {#each Array(10) as _, index}
          <span class={`size-3 rounded-full border ${index < 7 ? "border-[#d8ff57] bg-[#d8ff57]" : "border-[#52605a] bg-[#17201d]"}`}></span>
        {/each}
      </div>
    </div>

    <div class="grid gap-px border border-[#36423e] bg-[#36423e] min-[850px]:grid-cols-3">
      {#each tools as item}
        <a class="group grid min-h-64 grid-rows-[auto_1fr_auto] bg-[#111816] p-6 text-[#edf0e7] no-underline transition-colors hover:bg-[#18221e]" href={router.href(item.tool)} onclick={(event) => router.follow(event, item.tool)}>
          <div class="flex items-center justify-between font-['DM_Mono'] text-[10px] tracking-[.16em] uppercase">
            <span class="text-[#d8ff57]">{item.number}</span>
            <span class="text-[#64716b]">Entry / {item.name}</span>
          </div>
          <div class="self-center py-8">
            <h2 class="m-0 text-3xl font-semibold tracking-[-.03em]">{item.name}</h2>
            <p class="mt-3 mb-0 max-w-xs text-sm leading-6 text-[#929e98]">{item.description}</p>
          </div>
          <div class="flex items-center justify-between border-t border-[#303c37] pt-4 font-['DM_Mono'] text-[10px] tracking-[.1em] uppercase group-hover:text-[#d8ff57]">
            <span>{item.action}</span><span>→</span>
          </div>
        </a>
      {/each}
    </div>
  </div>
</section>
