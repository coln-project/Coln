<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { CompiledRealm } from "./compiled-realms.ts"
  import type { StoreRecord } from "./theory-document.ts"

  let {
    stores,
    realms,
    selectedRealmName,
    canCreate,
    creating,
    error,
    hint,
    onselect,
    oncreate,
    oncopy,
    onopen,
  }: {
    stores: StoreRecord[]
    realms: CompiledRealm[]
    selectedRealmName: string
    canCreate: boolean
    creating: boolean
    error: string
    hint: string
    onselect: (name: string) => void
    oncreate: () => void
    oncopy: (url: string) => void
    onopen: (store: StoreRecord) => void
  } = $props()

  function displayUrl(url: string): string {
    return url.length > 34 ? `${url.slice(0, 23)}…${url.slice(-8)}` : url
  }
</script>

<div class="pane-head">
  <div>
    <p class="label">Stores</p>
    <p class="hint">Stores created from compiled realms</p>
  </div>
  <span class="mono count">{stores.length.toString().padStart(2, "0")}</span>
</div>

<div class="create">
  {#if realms.length > 0}
    {#if realms.length > 1}
      <label class="realm-choice" for="coln-store-realm">
        <span class="label">Realm</span>
        <select
          class="field"
          id="coln-store-realm"
          value={selectedRealmName}
          onchange={event => onselect(event.currentTarget.value)}
        >
          <option value="">Choose a realm</option>
          {#each realms as realm (realm.name)}
            <option value={realm.name}>{realm.name}</option>
          {/each}
        </select>
      </label>
    {:else}
      <div class="realm-named">
        <span class="label">Realm</span>
        <strong class="mono">{realms[0].name}</strong>
      </div>
    {/if}

    <button
      class="action create-button"
      data-primary
      data-pending={creating || undefined}
      disabled={!canCreate}
      onclick={oncreate}
      data-testid="create-store"
    >
      {creating ? "Creating store…" : "Create store"}
      <span aria-hidden="true">+</span>
    </button>
    {#if hint}<p class="hint">{hint}</p>{/if}
  {:else}
    <p class="empty">Define and compile at least one realm before creating a store.</p>
  {/if}

  {#if error}<p class="alert" role="alert">{error}</p>{/if}
</div>

<div class="pane-body list">
  {#if stores.length === 0}
    <p class="empty">No stores created for this theory</p>
  {:else}
    {#each stores as store, index (store.url)}
      <article class="card" data-testid="store-record">
        <div class="card-head">
          <div class="card-title">
            <p class="realm-name mono truncate">{store.realmName}</p>
            <time
              class="mono muted"
              datetime={new Date(store.createdAt).toISOString()}
            >{new Date(store.createdAt).toLocaleString()}</time>
          </div>
          <span class="mono muted">{String(stores.length - index).padStart(2, "0")}</span>
        </div>
        <code class="mono truncate" title={store.url}>{displayUrl(store.url)}</code>
        <span class="mono muted">
          {store.sourceHeads.length} source {store.sourceHeads.length === 1 ? "head" : "heads"}
        </span>
        <div class="cluster card-actions">
          <button class="action" onclick={() => onopen(store)}>Open store</button>
          <button class="action" onclick={() => oncopy(store.url)}>Copy url</button>
        </div>
      </article>
    {/each}
  {/if}
</div>

<style>
  .count,
  .muted {
    color: var(--coln-faint);
  }

  .create {
    border-bottom: 1px solid var(--coln-border);
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
    padding: var(--studio-space-md, 1rem);
  }

  .realm-choice {
    display: grid;
    gap: 0.375rem;
  }

  .realm-named {
    border-left: 2px solid var(--coln-accent);
    display: grid;
    gap: 0.25rem;
    padding-left: var(--studio-space-sm, 0.75rem);
  }

  .create-button {
    align-items: center;
    display: flex;
    justify-content: space-between;
    padding: 0.625rem 0.875rem;
  }

  .list {
    align-content: start;
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
  }

  .card-head {
    align-items: flex-start;
    display: flex;
    gap: var(--studio-space-sm, 0.75rem);
    justify-content: space-between;
  }

  .card-title {
    min-width: 0;
  }

  .realm-name {
    color: var(--coln-accent-text);
    font-weight: 500;
    margin: 0;
  }

  .card-actions {
    justify-content: flex-end;
  }
</style>
