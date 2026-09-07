<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { RowView, Value } from "@coln-project/runtime"
  import { tick, type Snippet } from "svelte"
  import { displayRowRef, displayValue } from "./format.ts"
  import type { StoreTable } from "./schema.ts"

  let {
    tables,
    selected,
    rows,
    selectedRowId,
    referenceNavigation,
    onselect,
    canfollow,
    onfollow,
    toolbar,
  }: {
    tables: StoreTable[]
    selected: StoreTable | undefined
    rows: RowView[]
    selectedRowId: string
    referenceNavigation: number
    onselect: (name: string) => void
    canfollow: (tableName: string | undefined, value: Value) => boolean
    onfollow: (tableName: string | undefined, value: Value) => void
    toolbar?: Snippet
  } = $props()

  let filter = $state("")
  let grid: HTMLDivElement
  const visibleTables = $derived(
    tables.filter((table) => table.name.toLowerCase().includes(filter.toLowerCase())),
  )

  $effect(() => {
    const navigation = referenceNavigation
    const target = selectedRowId
    if (!target) return
    filter = ""

    void tick().then(() => {
      if (navigation !== referenceNavigation || target !== selectedRowId) return
      const row = grid?.querySelector<HTMLElement>('[data-selected="true"]')
      row?.scrollIntoView({ block: "nearest", inline: "nearest" })
      row?.focus()
    })
  })
</script>

<section class="flex h-full min-h-0 flex-col border-b border-[#304041] bg-[#101718] min-[761px]:border-r min-[761px]:border-b-0">
  <div class="grid gap-3 border-b border-[#304041] p-4 min-[761px]:p-5">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <p class="m-0 font-['DM_Mono'] text-xs tracking-[.16em] text-[#748284]" data-small-detail>STORE CONTENTS</p>
        <p class="mt-1 mb-0 text-sm text-[#91a0a1]">{tables.length} {tables.length === 1 ? "table" : "tables"}</p>
      </div>
      {#if toolbar}<div class="min-w-0 flex-1">{@render toolbar()}</div>{/if}
    </div>
    <input
      class="h-9 min-w-0 border border-[#304041] bg-[#0b1112] px-3 font-['DM_Mono'] text-sm text-[#e8ece8] outline-none placeholder:text-[#536163] focus:border-[#d8ff57]"
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
          class={`shrink-0 cursor-pointer border-0 px-3 py-2.5 text-left font-['DM_Mono'] text-sm ${selected?.name === table.name ? "bg-[#d8ff57] text-[#101718]" : "bg-[#131b1c] text-[#aab6b7] hover:bg-[#182122] hover:text-[#d8ff57]"}`}
          data-testid="table-option"
          data-selected={selected?.name === table.name}
          onclick={() => onselect(table.name)}
        >
          <span class="block truncate">{table.name}</span>
          <span class={`mt-1 block text-sm ${selected?.name === table.name ? "text-[#42501d]" : "text-[#667576]"}`}>{table.columns.length} {table.columns.length === 1 ? "column" : "columns"}</span>
        </button>
      {:else}
        <p class="m-0 bg-[#131b1c] p-3 font-['DM_Mono'] text-sm text-[#667576]">No tables match this filter.</p>
      {/each}
    </nav>

    <div class="min-w-0 overflow-auto bg-[#0b1112]" data-testid="table-grid" bind:this={grid}>
      {#if selected}
        <table class="w-max min-w-full border-collapse font-['DM_Mono'] text-sm">
          <thead class="sticky top-0 z-10 bg-[#182122] text-left text-sm tracking-[.08em] text-[#839193] uppercase">
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
              <tr
                class={`border-b border-[#253233] outline-none hover:bg-[#111a1b] ${selectedRowId === rowId.full ? "bg-[#182627] ring-1 ring-inset ring-[#d8ff57]" : ""}`}
                data-testid="table-row"
                data-selected={selectedRowId === rowId.full}
                tabindex="-1"
              >
                <td class="border-r border-[#253233] px-3 py-2.5 text-[#77b7ff]" title={rowId.full}>{rowId.compact}</td>
                {#each row.values as value, index}
                  {@const displayed = displayValue(value)}
                  <td class={`max-w-80 border-r border-[#253233] px-3 py-2.5 last:border-r-0 ${displayed.kind === "string" ? "text-[#e8ece8]" : displayed.kind === "int" ? "text-[#ffd166]" : displayed.kind === "ref" ? "text-[#77b7ff]" : "text-[#ff9a86]"}`} title={displayed.full}>
                    {#if displayed.kind === "ref" && canfollow(selected.columns[index]?.referenceTable, value)}
                      <button
                        type="button"
                        class="block max-w-full cursor-pointer overflow-hidden border-0 bg-transparent p-0 font-inherit text-inherit underline decoration-[#44678c] underline-offset-2 hover:text-[#d8ff57] focus-visible:text-[#d8ff57] focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-[#d8ff57]"
                        aria-label={`Open referenced row ${displayed.full}`}
                        data-testid="table-reference"
                        onclick={() => onfollow(selected.columns[index]?.referenceTable, value)}
                      >
                        <span class="block overflow-hidden text-ellipsis whitespace-nowrap">{displayed.compact}</span>
                      </button>
                    {:else}
                      <span class="block overflow-hidden text-ellipsis whitespace-nowrap">{displayed.compact}</span>
                    {/if}
                  </td>
                {/each}
              </tr>
            {:else}
              <tr>
                <td colspan={selected.columns.length + 1} class="px-4 py-8 text-center text-[#667576]">No rows in {selected.name}.</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="grid min-h-72 place-items-center p-8 text-center font-['DM_Mono'] text-sm text-[#667576]">This store has no tables.</div>
      {/if}
    </div>
  </div>
</section>
