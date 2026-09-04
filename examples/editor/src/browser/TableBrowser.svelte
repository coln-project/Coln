<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { RowView } from "@coln-project/runtime"
  import { displayRowRef, displayValue } from "../lib/format.ts"
  import type { StoreTable } from "../lib/schema.ts"

  let {
    tables,
    selected,
    rows,
    onselect,
  }: {
    tables: StoreTable[]
    selected: StoreTable | undefined
    rows: RowView[]
    onselect: (name: string) => void
  } = $props()

  let filter = $state("")
  const visibleTables = $derived(
    tables.filter((table) => table.name.toLowerCase().includes(filter.toLowerCase())),
  )
</script>

<section class="flex min-h-0 flex-col border-b border-[#304041] bg-[#101718] min-[761px]:border-r min-[761px]:border-b-0">
  <div class="grid gap-3 border-b border-[#304041] p-4 min-[1050px]:grid-cols-[minmax(160px,220px)_1fr] min-[761px]:p-5">
    <div>
      <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">STORE CONTENTS</p>
      <p class="mt-1 mb-0 text-xs text-[#91a0a1]">{tables.length} {tables.length === 1 ? "table" : "tables"}</p>
    </div>
    <input
      class="h-9 min-w-0 border border-[#304041] bg-[#0b1112] px-3 font-['DM_Mono'] text-xs text-[#e8ece8] outline-none placeholder:text-[#536163] focus:border-[#d8ff57]"
      aria-label="Filter tables"
      data-testid="table-filter"
      placeholder="Filter tables"
      bind:value={filter}
    />
  </div>

  <div class="grid min-h-0 flex-1 grid-rows-[auto_minmax(320px,1fr)] min-[1050px]:grid-cols-[220px_minmax(0,1fr)] min-[1050px]:grid-rows-1">
    <nav class="flex max-h-44 gap-px overflow-auto border-b border-[#304041] bg-[#253233] p-px min-[1050px]:max-h-none min-[1050px]:flex-col min-[1050px]:border-r min-[1050px]:border-b-0" aria-label="Store tables">
      {#each visibleTables as table (table.name)}
        <button
          class={`shrink-0 cursor-pointer border-0 px-3 py-2.5 text-left font-['DM_Mono'] text-[11px] ${selected?.name === table.name ? "bg-[#d8ff57] text-[#101718]" : "bg-[#131b1c] text-[#aab6b7] hover:bg-[#182122] hover:text-[#d8ff57]"}`}
          data-testid="table-option"
          data-selected={selected?.name === table.name}
          onclick={() => onselect(table.name)}
        >
          <span class="block truncate">{table.name}</span>
          <span class={`mt-1 block text-[9px] ${selected?.name === table.name ? "text-[#42501d]" : "text-[#667576]"}`}>{table.columns.length} cols</span>
        </button>
      {:else}
        <p class="m-0 bg-[#131b1c] p-3 font-['DM_Mono'] text-[10px] text-[#667576]">No matching tables</p>
      {/each}
    </nav>

    <div class="min-w-0 overflow-auto bg-[#0b1112]" data-testid="table-grid">
      {#if selected}
        <table class="w-max min-w-full border-collapse font-['DM_Mono'] text-[11px]">
          <thead class="sticky top-0 z-10 bg-[#182122] text-left text-[9px] tracking-[.1em] text-[#839193] uppercase">
            <tr>
              <th class="border-r border-b border-[#304041] px-3 py-3 font-medium">Row ID</th>
              {#each selected.columns as column}
                <th class="border-r border-b border-[#304041] px-3 py-3 font-medium last:border-r-0">
                  <span class="text-[#aab6b7]">{column.name}{column.primary ? " *" : ""}</span>
                  <span class="mt-1 block normal-case tracking-normal text-[#536163]">{column.type}</span>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as row (displayRowRef(row.rowId.value).full)}
              {@const rowId = displayRowRef(row.rowId.value)}
              <tr class="border-b border-[#253233] hover:bg-[#111a1b]" data-testid="table-row">
                <td class="border-r border-[#253233] px-3 py-2.5 text-[#77b7ff]" title={rowId.full}>{rowId.compact}</td>
                {#each row.values as value}
                  {@const displayed = displayValue(value)}
                  <td class={`max-w-80 border-r border-[#253233] px-3 py-2.5 last:border-r-0 ${displayed.kind === "string" ? "text-[#e8ece8]" : displayed.kind === "int" ? "text-[#ffd166]" : displayed.kind === "ref" ? "text-[#77b7ff]" : "text-[#ff9a86]"}`} title={displayed.full}>
                    <span class="block overflow-hidden text-ellipsis whitespace-nowrap">{displayed.compact}</span>
                  </td>
                {/each}
              </tr>
            {:else}
              <tr>
                <td colspan={selected.columns.length + 1} class="px-4 py-8 text-center text-[#667576]">No rows in {selected.name}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="grid min-h-72 place-items-center p-8 text-center font-['DM_Mono'] text-xs text-[#667576]">This store has no tables.</div>
      {/if}
    </div>
  </div>
</section>
