<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { untrack } from "svelte"
  import { syncEndpoint } from "./repo.ts"

  let {
    heading,
    blurb,
    initialUrl = "",
    loading = false,
    error = "",
    createLabel = "",
    onopen,
    oncreate,
  }: {
    heading: string
    blurb: string
    initialUrl?: string
    loading?: boolean
    error?: string
    createLabel?: string
    onopen: (url: string) => void
    oncreate?: () => void
  } = $props()

  // Seeds the field once; later prop changes are the parent's business.
  let storeUrl = $state(untrack(() => initialUrl))
</script>

<section class="setup">
  <form
    class="form"
    onsubmit={event => {
      event.preventDefault()
      onopen(storeUrl)
    }}
  >
    <div class="intro">
      <p class="label">Coln store</p>
      <h1>{heading}</h1>
      <p class="hint">{blurb}</p>
    </div>

    <div class="controls">
      <input
        class="field"
        aria-label="Coln store url"
        data-testid="store-url-input"
        placeholder="coln:…"
        autocomplete="off"
        bind:value={storeUrl}
      />
      <button
        class="action"
        data-primary
        data-pending
        disabled={loading}
        data-testid="open-store"
      >{loading ? "Opening…" : "Open store"}</button>
      {#if error}
        <p class="alert wide" role="alert" data-testid="load-error">{error}</p>
      {/if}
    </div>

    {#if oncreate}
      <div class="create">
        <div>
          <p class="label">Or start fresh</p>
          <p class="hint">Creates a new store on {syncEndpoint()}</p>
        </div>
        <button
          class="action"
          type="button"
          disabled={loading}
          onclick={oncreate}
          data-testid="create-store"
        >{createLabel || "Create store"}</button>
      </div>
    {/if}
  </form>
</section>

<style>
  .setup {
    display: grid;
    height: 100%;
    min-height: 0;
    overflow: auto;
    padding: var(--studio-space-md, 1rem);
    place-items: center;
  }

  .form {
    background: var(--coln-fill);
    border: 1px solid var(--coln-border);
    border-radius: var(--studio-radius-md, 8px);
    max-width: 36rem;
    width: 100%;
  }

  .intro {
    border-bottom: 1px solid var(--coln-border);
    padding: var(--studio-space-lg, 1.5rem);
  }

  .intro h1 {
    font-size: 1.5rem;
    letter-spacing: -0.02em;
    margin: var(--studio-space-sm, 0.75rem) 0 var(--studio-space-xs, 0.375rem);
  }

  .controls {
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
    padding: var(--studio-space-lg, 1.5rem);
  }

  @media (min-width: 600px) {
    .controls {
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .wide {
      grid-column: 1 / -1;
    }
  }

  .create {
    align-items: end;
    border-top: 1px solid var(--coln-border);
    display: grid;
    gap: var(--studio-space-sm, 0.75rem);
    padding: var(--studio-space-lg, 1.5rem);
  }

  @media (min-width: 600px) {
    .create {
      grid-template-columns: minmax(0, 1fr) auto;
    }
  }
</style>
