<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { StoreTable } from "../store/schema.ts"

  let { tables, headCount }: {
    tables: Array<StoreTable & { rowCount: number }>
    headCount: number
  } = $props()

  const rowCount = $derived(
    tables.reduce((total, table) => total + table.rowCount, 0),
  )
</script>

<section class="summary" aria-labelledby="coln-store-summary" data-testid="store-summary">
  <div class="head">
    <div>
      <p class="label" id="coln-store-summary">Store summary</p>
      <p class="hint">Tables backing this graph</p>
    </div>
    <span class="mono faint">{headCount} {headCount === 1 ? "head" : "heads"}</span>
  </div>

  <div class="totals">
    <div><strong>{tables.length}</strong><span class="label">tables</span></div>
    <div><strong>{rowCount}</strong><span class="label">rows</span></div>
    <div><strong>{headCount}</strong><span class="label">heads</span></div>
  </div>

  <div class="tables">
    {#each tables as table (table.name)}
      <article class="card" data-testid="store-summary-table" data-table-name={table.name}>
        <div class="table-head">
          <strong class="mono truncate">{table.name}</strong>
          <span class="mono accent" data-testid="store-summary-row-count">
            {table.rowCount} {table.rowCount === 1 ? "row" : "rows"}
          </span>
        </div>
        {#if table.columns.length === 0}
          <p class="empty">No non-key value columns</p>
        {:else}
          {#each table.columns as column (column.name)}
            <div class="column">
              <span class="mono">{column.name}{column.primary ? " *" : ""}</span>
              <span class="mono faint truncate">{column.type}</span>
            </div>
          {/each}
        {/if}
      </article>
    {/each}
  </div>
</section>

<style>
  .summary {
    border-top: 1px solid var(--coln-border);
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
    padding: var(--studio-space-sm, 0.75rem);
  }

  .head {
    align-items: end;
    display: flex;
    gap: var(--studio-space-sm, 0.75rem);
    justify-content: space-between;
  }

  .faint {
    color: var(--coln-faint);
  }

  .accent {
    color: var(--coln-accent-text);
  }

  .totals {
    background: var(--coln-fill);
    border: 1px solid var(--coln-border);
    border-radius: var(--studio-radius-sm, 4px);
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    text-align: center;
  }

  .totals > div {
    border-right: 1px solid var(--coln-border);
    padding: 0.5rem;
  }

  .totals > div:last-child {
    border-right: 0;
  }

  .totals strong {
    color: var(--coln-accent-text);
    display: block;
    font-family: var(--coln-mono);
    font-size: 0.875rem;
  }

  .totals .label {
    font-size: 0.6875rem;
    letter-spacing: 0.06em;
  }

  .tables {
    display: grid;
    gap: var(--studio-space-xs, 0.375rem);
  }

  .table-head {
    align-items: center;
    display: flex;
    gap: var(--studio-space-sm, 0.75rem);
    justify-content: space-between;
  }

  .column {
    display: flex;
    gap: var(--studio-space-sm, 0.75rem);
    justify-content: space-between;
  }
</style>
