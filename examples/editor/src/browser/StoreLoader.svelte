<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let {
    loading,
    error,
    onload,
  }: {
    loading: boolean
    error: string
    onload: (url: string) => void
  } = $props()

  let documentUrl = $state("")
</script>

<main class="grid min-h-screen min-w-80 grid-rows-[56px_1fr] bg-[#101718] font-['Manrope'] text-[#e8ece8] min-[761px]:grid-rows-[64px_1fr]">
  <header class="flex items-center border-b border-[#304041] px-4 min-[761px]:px-6">
    <div class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.13em]">
      <span class="grid size-7.5 place-items-center border border-[#d8ff57] text-base text-[#d8ff57]">C</span>
      <span>COLN / STORE LAB</span>
    </div>
  </header>
  <section class="grid place-items-center p-5">
    <form
      class="w-full max-w-2xl border border-[#304041] bg-[#131b1c]"
      onsubmit={(event) => {
        event.preventDefault()
        onload(documentUrl)
      }}
    >
      <div class="border-b border-[#304041] p-5 min-[761px]:p-7">
        <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#748284]">01 / OPEN STORE</p>
        <h1 class="mt-4 mb-2 text-2xl font-semibold tracking-[-.02em] min-[761px]:text-3xl">Inspect a synchronized Coln store.</h1>
        <p class="m-0 max-w-xl text-sm leading-relaxed text-[#91a0a1]">Enter an Automerge document URL. Store Lab will resolve its Coln schema, tables, and live rows.</p>
      </div>
      <div class="grid gap-3 p-5 min-[761px]:grid-cols-[1fr_auto] min-[761px]:p-7">
        <input
          class="h-11 min-w-0 border border-[#6b7a7b] bg-[#0b1112] px-3 font-['DM_Mono'] text-xs text-[#e8ece8] outline-none placeholder:text-[#536163] focus:border-[#d8ff57]"
          aria-label="Automerge document URL"
          data-testid="document-url-input"
          placeholder="automerge:…"
          autocomplete="off"
          bind:value={documentUrl}
        />
        <button class="h-11 cursor-pointer border border-[#d8ff57] bg-[#d8ff57] px-5 font-['DM_Mono'] text-[10px] font-medium tracking-[.12em] text-[#101718] uppercase hover:bg-transparent hover:text-[#d8ff57] disabled:cursor-wait disabled:opacity-40" disabled={loading} data-testid="open-store">
          {loading ? "Loading…" : "Open store"}
        </button>
        {#if error}
          <p class="m-0 border-l-2 border-[#ff7657] bg-[#211918] p-3 font-['DM_Mono'] text-xs text-[#ff9a86] min-[761px]:col-span-2" role="alert" data-testid="load-error">{error}</p>
        {/if}
      </div>
    </form>
  </section>
</main>
