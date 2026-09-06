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
    openhref,
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
    openhref?: (url: string) => string
    onopen?: (event: MouseEvent, url: string) => void
  } = $props()

  function displayUrl(url: string): string {
    return url.length > 34 ? `${url.slice(0, 23)}...${url.slice(-8)}` : url
  }
</script>

<aside class="flex min-h-0 flex-col border-t border-[#304041] bg-[#182122] min-[761px]:col-span-2 min-[1100px]:col-span-1 min-[1100px]:border-t-0 min-[1100px]:border-l">
  <header class="flex min-h-16 items-center justify-between border-b border-[#304041] px-4">
    <div>
      <p class="m-0 font-['DM_Mono'] text-[10px] tracking-[.16em] text-[#91a0a1]">STORES</p>
      <p class="mt-1 mb-0 text-xs text-[#748284]">Compiled instances</p>
    </div>
    <span class="font-['DM_Mono'] text-[10px] text-[#667576]">{stores.length.toString().padStart(2, "0")}</span>
  </header>

  <div class="grid gap-4 border-b border-[#304041] p-4">
    {#if realms.length > 0}
      {#if realms.length > 1}
        <label class="grid gap-1.5 font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#91a0a1] uppercase" for="store-realm">
          Realm
          <select
            class="h-10 w-full rounded-none border border-[#304041] bg-[#101718] px-2.5 text-xs normal-case text-[#e8ece8] outline-none focus:border-[#d8ff57]"
            id="store-realm"
            value={selectedRealmName}
            onchange={event => onselect(event.currentTarget.value)}
          >
            <option value="">Choose realm</option>
            {#each realms as realm}
              <option value={realm.name}>{realm.name}</option>
            {/each}
          </select>
        </label>
      {:else}
        <div class="grid gap-1 border-l-2 border-[#d8ff57] pl-3">
          <span class="font-['DM_Mono'] text-[9px] tracking-[.12em] text-[#839193] uppercase">Realm</span>
          <strong class="font-['DM_Mono'] text-xs font-medium text-[#e8ece8]">{realms[0].name}</strong>
        </div>
      {/if}

      <button
        class="flex h-11 cursor-pointer items-center justify-between border-0 bg-[#d8ff57] px-3.5 font-bold text-[#101718] disabled:cursor-not-allowed disabled:opacity-35"
        disabled={!canCreate}
        onclick={oncreate}
      >
        {creating ? "Creating store" : "Create empty store"}
        <span class="font-['DM_Mono'] text-xl">+</span>
      </button>
      {#if hint}<p class="m-0 text-[11px] leading-relaxed text-[#748284]">{hint}</p>{/if}
    {:else}
      <p class="m-0 text-xs leading-relaxed text-[#667576]">Compile a theory containing a realm to create a store.</p>
    {/if}

    {#if error}<p class="m-0 font-['DM_Mono'] text-[11px] leading-relaxed text-[#ff9a86]" role="alert">{error}</p>{/if}
  </div>

  <div class="grid content-start gap-3 overflow-auto p-4">
    {#if stores.length === 0}
      <p class="m-0 text-xs text-[#667576]">No stores created</p>
    {:else}
      {#each stores as store, index}
        <article class="grid gap-2 border border-[#304041] bg-[#101718] p-3">
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <p class="m-0 truncate font-['DM_Mono'] text-xs font-medium text-[#d8ff57]">{store.realmName}</p>
              <time class="font-['DM_Mono'] text-[9px] text-[#667576]" datetime={new Date(store.createdAt).toISOString()}>
                {new Date(store.createdAt).toLocaleString()}
              </time>
            </div>
            <span class="font-['DM_Mono'] text-[9px] text-[#536163]">{String(stores.length - index).padStart(2, "0")}</span>
          </div>
          <code class="overflow-hidden text-ellipsis whitespace-nowrap font-['DM_Mono'] text-[9px] text-[#839193]" title={store.url}>{displayUrl(store.url)}</code>
          <div class="flex items-center justify-between gap-2">
            <span class="font-['DM_Mono'] text-[9px] text-[#667576]">{store.sourceHeads.length} source {store.sourceHeads.length === 1 ? "head" : "heads"}</span>
            <div class="flex items-center gap-1.5">
              {#if openhref}
                <a class="border border-[#4a5a5b] px-2 py-1 font-['DM_Mono'] text-[9px] text-[#e8ece8] no-underline hover:border-[#d8ff57] hover:text-[#d8ff57]" href={openhref(store.url)} target={onopen ? undefined : "_blank"} rel={onopen ? undefined : "noreferrer"} onclick={(event) => onopen?.(event, store.url)}>Open</a>
              {/if}
              <button class="cursor-pointer border border-[#4a5a5b] bg-transparent px-2 py-1 font-['DM_Mono'] text-[9px] text-[#e8ece8] hover:border-[#d8ff57] hover:text-[#d8ff57]" onclick={() => oncopy(store.url)}>Copy URL</button>
            </div>
          </div>
        </article>
      {/each}
    {/if}
  </div>
</aside>
