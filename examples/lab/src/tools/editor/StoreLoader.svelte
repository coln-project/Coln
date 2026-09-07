<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { onMount } from "svelte"

  let { initialUrl, recentUrl, loading, error, onload }: {
    initialUrl: string
    recentUrl: string
    loading: boolean
    error: string
    onload: (url: string) => void
  } = $props()

  let documentUrl = $state("")

  onMount(() => {
    documentUrl = initialUrl
  })

  function displayUrl(url: string): string {
    return url.length > 48 ? `${url.slice(0, 34)}...${url.slice(-10)}` : url
  }
</script>

<section class="lab-tool grid h-full min-h-0 place-items-center overflow-auto p-5">
  <form
    class="w-full max-w-2xl border border-[#304041] bg-[#131b1c]"
    onsubmit={(event) => {
      event.preventDefault()
      onload(documentUrl)
    }}
  >
    <div class="border-b border-[#304041] p-5 min-[761px]:p-7">
      <p class="m-0 font-['DM_Mono'] text-xs tracking-[.16em] text-[#748284]" data-small-detail>STORE EDITOR / OPEN STORE</p>
      <h1 class="mt-4 mb-2 text-2xl font-semibold tracking-[-.02em] min-[761px]:text-3xl">Inspect a Coln store.</h1>
      <p class="m-0 max-w-xl text-sm leading-relaxed text-[#91a0a1]">Enter a Coln store URL to inspect its schema, tables, rows, and references.</p>
    </div>
    <div class="grid gap-3 p-5 min-[761px]:grid-cols-[1fr_auto] min-[761px]:p-7">
      <input
        class="h-11 min-w-0 border border-[#6b7a7b] bg-[#0b1112] px-3 font-['DM_Mono'] text-sm text-[#e8ece8] outline-none placeholder:text-[#536163] focus:border-[#d8ff57]"
        aria-label="Coln store URL"
        data-testid="document-url-input"
        placeholder="coln:…"
        autocomplete="off"
        bind:value={documentUrl}
      />
      <button class="lab-primary-action lab-pending-action h-11 px-5 font-['DM_Mono'] text-sm font-medium tracking-[.12em] uppercase" disabled={loading} data-testid="open-store">
        {loading ? "Opening store…" : "Open in Store Editor"}
      </button>
      {#if error}
        <p class="lab-alert m-0 p-3 min-[761px]:col-span-2" role="alert" data-testid="load-error">{error}</p>
      {/if}
    </div>
    {#if recentUrl}
      <div class="grid gap-3 border-t border-[#304041] p-5 min-[761px]:grid-cols-[1fr_auto] min-[761px]:items-end min-[761px]:p-7" data-testid="recent-store">
        <div class="min-w-0">
          <p class="m-0 font-['DM_Mono'] text-xs tracking-[.14em] text-[#748284] uppercase" data-small-detail>Recent store</p>
          <code class="mt-2 block overflow-hidden text-ellipsis whitespace-nowrap font-['DM_Mono'] text-sm text-[#91a0a1]" title={recentUrl}>{displayUrl(recentUrl)}</code>
        </div>
        <button class="lab-secondary-action" type="button" onclick={() => onload(recentUrl)} disabled={loading}>Reopen store</button>
      </div>
    {/if}
  </form>
</section>
