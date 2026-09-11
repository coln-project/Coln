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
    tables.filter(table =>
      table.name.toLowerCase().includes(filter.toLowerCase()),
    ),
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

<section class="browser">
  <div class="pane-head">
    <div>
      <p class="label">Store contents</p>
      <p class="hint">{tables.length} {tables.length === 1 ? "table" : "tables"}</p>
    </div>
    {#if toolbar}<div class="toolbar">{@render toolbar()}</div>{/if}
    <input
      class="field filter"
      aria-label="Filter tables"
      data-testid="table-filter"
      placeholder="Filter tables"
      bind:value={filter}
    />
  </div>

  <div class="split">
    <nav class="tables" aria-label="Store tables">
      {#each visibleTables as table (table.name)}
        <button
          class="table-option"
          data-testid="table-option"
          data-selected={selected?.name === table.name}
          onclick={() => onselect(table.name)}
        >
          <span class="truncate">{table.name}</span>
          <span class="columns">
            {table.columns.length} {table.columns.length === 1 ? "column" : "columns"}
          </span>
        </button>
      {:else}
        <p class="empty no-match">No tables match this filter.</p>
      {/each}
    </nav>

    <div class="grid" data-testid="table-grid" bind:this={grid}>
      {#if selected}
        <table>
          <thead>
            <tr>
              <th>Row ID</th>
              {#each selected.columns as column}
                <th>
                  <span class="column-name">{column.name}{column.primary ? " *" : ""}</span>
                  <span class="column-type">{column.type}</span>
                </th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as row (displayRowRef(row.rowId.value).full)}
              {@const rowId = displayRowRef(row.rowId.value)}
              <tr
                data-testid="table-row"
                data-selected={selectedRowId === rowId.full}
                tabindex="-1"
              >
                <td class="row-id" title={rowId.full}>{rowId.compact}</td>
                {#each row.values as value, index}
                  {@const displayed = displayValue(value)}
                  <td data-kind={displayed.kind} title={displayed.full}>
                    {#if displayed.kind === "ref" && canfollow(selected.columns[index]?.referenceTable, value)}
                      <button
                        class="reference"
                        type="button"
                        aria-label={`Open referenced row ${displayed.full}`}
                        data-testid="table-reference"
                        onclick={() => onfollow(selected.columns[index]?.referenceTable, value)}
                      ><span class="truncate">{displayed.compact}</span></button>
                    {:else}
                      <span class="truncate">{displayed.compact}</span>
                    {/if}
                  </td>
                {/each}
              </tr>
            {:else}
              <tr>
                <td class="no-rows" colspan={selected.columns.length + 1}>
                  No rows in {selected.name}.
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="empty no-tables">This store has no tables.</p>
      {/if}
    </div>
  </div>
</section>

<style>
  .browser {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .toolbar {
    flex: 1;
    min-width: 0;
  }

  .filter {
    flex-basis: 100%;
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-rows: auto minmax(0, 1fr);
    min-height: 0;
  }

  @media (min-width: 1100px) {
    .split {
      grid-template-columns: 13rem minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr);
    }
  }

  .tables {
    background: var(--coln-surface-raised);
    border-bottom: 1px solid var(--coln-border);
    display: flex;
    gap: 1px;
    max-height: 11rem;
    overflow: auto;
  }

  @media (min-width: 1100px) {
    .tables {
      border-bottom: 0;
      border-right: 1px solid var(--coln-border);
      flex-direction: column;
      max-height: none;
    }
  }

  .table-option {
    background: var(--coln-fill);
    border: 0;
    color: var(--coln-muted);
    cursor: pointer;
    display: grid;
    flex-shrink: 0;
    font-family: var(--coln-mono);
    font-size: 0.875rem;
    gap: 0.25rem;
    padding: 0.625rem var(--studio-space-sm, 0.75rem);
    text-align: left;
  }

  .table-option:hover {
    color: var(--coln-accent-text);
  }

  .table-option[data-selected="true"] {
    background: var(--coln-accent);
    color: var(--coln-accent-line);
  }

  .columns {
    font-size: 0.8125rem;
    opacity: 0.7;
  }

  .no-match,
  .no-tables {
    padding: var(--studio-space-sm, 0.75rem);
  }

  .no-tables {
    display: grid;
    place-items: center;
  }

  .grid {
    background: var(--coln-fill);
    min-width: 0;
    overflow: auto;
  }

  table {
    border-collapse: collapse;
    font-family: var(--coln-mono);
    font-size: 0.875rem;
    min-width: 100%;
    width: max-content;
  }

  thead {
    background: var(--coln-surface-raised);
    position: sticky;
    text-align: left;
    top: 0;
    z-index: 1;
  }

  th {
    border-bottom: 1px solid var(--coln-border);
    border-right: 1px solid var(--coln-border);
    font-weight: 500;
    padding: 0.625rem var(--studio-space-sm, 0.75rem);
  }

  th:last-child {
    border-right: 0;
  }

  .column-name {
    color: var(--coln-line);
    font-size: 0.8125rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .column-type {
    color: var(--coln-faint);
    display: block;
    margin-top: 0.25rem;
  }

  tbody tr {
    border-bottom: 1px solid var(--coln-border);
    outline: none;
  }

  tbody tr:hover {
    background: var(--coln-surface);
  }

  tbody tr[data-selected="true"] {
    background: var(--coln-selected-fill);
    box-shadow: inset 0 0 0 1px var(--coln-accent);
  }

  td {
    border-right: 1px solid var(--coln-border);
    max-width: 20rem;
    padding: 0.5rem var(--studio-space-sm, 0.75rem);
  }

  td:last-child {
    border-right: 0;
  }

  td[data-kind="int"] { color: var(--coln-warning-text); }
  td[data-kind="ref"] { color: var(--coln-link-text); }
  td[data-kind="unknown"] { color: var(--coln-danger-text); }

  .row-id {
    color: var(--coln-link-text);
  }

  .no-rows {
    color: var(--coln-faint);
    padding: var(--studio-space-lg, 2rem);
    text-align: center;
  }

  .reference {
    background: none;
    border: 0;
    color: inherit;
    cursor: pointer;
    display: block;
    font: inherit;
    max-width: 100%;
    overflow: hidden;
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .reference:hover,
  .reference:focus-visible {
    color: var(--coln-accent-text);
  }
</style>
