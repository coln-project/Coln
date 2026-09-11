<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { StoreTable } from "./schema.ts"

  let { tables, headCount }: {
    tables: Array<StoreTable & { rowCount: number }>
    headCount: number
  } = $props()

  const rowCount = $derived(tables.reduce((total, table) => total + table.rowCount, 0))
</script>

<section class="grid gap-2 border-t border-[#304041] p-3" aria-labelledby="store-summary-heading" data-testid="store-summary">
  <div class="flex items-end justify-between gap-4">
    <div>
      <p class="m-0 font-['DM_Mono'] text-xs tracking-[.16em] text-[#748284]" id="store-summary-heading" data-small-detail>STORE SUMMARY</p>
      <p class="mt-1 mb-0 text-sm text-[#91a0a1]">Tables backing this graph</p>
    </div>
    <span class="font-['DM_Mono'] text-sm text-[#667576]">{headCount} {headCount === 1 ? "head" : "heads"}</span>
  </div>

  <div class="grid grid-cols-3 border border-[#304041] bg-[#101718] font-['DM_Mono'] text-center">
    <div class="border-r border-[#304041] p-2.5"><strong class="block text-sm text-[#d8ff57]">{tables.length}</strong><span class="text-xs tracking-[.06em] text-[#748284] uppercase" data-small-detail>tables</span></div>
    <div class="border-r border-[#304041] p-2.5"><strong class="block text-sm text-[#d8ff57]">{rowCount}</strong><span class="text-xs tracking-[.06em] text-[#748284] uppercase" data-small-detail>rows</span></div>
    <div class="p-2.5"><strong class="block text-sm text-[#d8ff57]">{headCount}</strong><span class="text-xs tracking-[.06em] text-[#748284] uppercase" data-small-detail>heads</span></div>
  </div>

  <div class="grid gap-2 min-[761px]:grid-cols-2">
    {#each tables as table}
      <article class="grid gap-2 border border-[#304041] bg-[#101718] p-3" data-testid="store-summary-table" data-table-name={table.name}>
        <div class="flex items-center justify-between gap-3">
          <strong class="truncate font-['DM_Mono'] text-sm font-medium text-[#e8ece8]">{table.name}</strong>
          <span class="shrink-0 font-['DM_Mono'] text-sm text-[#d8ff57]" data-testid="store-summary-row-count">{table.rowCount} {table.rowCount === 1 ? "row" : "rows"}</span>
        </div>
        {#if table.columns.length === 0}
          <p class="m-0 font-['DM_Mono'] text-sm text-[#667576]">No non-key value columns</p>
        {:else}
          <div class="grid gap-1">
            {#each table.columns as column}
              <div class="flex items-center justify-between gap-3 font-['DM_Mono'] text-sm">
                <span class="text-[#91a0a1]">{column.name}{column.primary ? " *" : ""}</span>
                <span class="truncate text-right text-[#667576]">{column.type}</span>
              </div>
            {/each}
          </div>
        {/if}
      </article>
    {/each}
  </div>
</section>
