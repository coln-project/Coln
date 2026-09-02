<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let { documentUrl, kind }: {
    documentUrl: string
    kind: "invalid" | "unavailable"
  } = $props()

  const title = $derived(kind === "invalid" ? "Document URL invalid" : "Document unavailable")
  const heading = $derived(
    kind === "invalid"
      ? "This is not a valid Automerge document URL."
      : "This graph could not be found.",
  )
  const detail = $derived(
    kind === "invalid"
      ? "Check the URL and try again, or create a new graph."
      : "It may not have been synchronized yet, or the URL may point to a document that no longer exists.",
  )

  function createNewGraph(event: MouseEvent) {
    event.preventDefault()
    history.replaceState(null, "", `${location.pathname}${location.search}`)
    location.reload()
  }
</script>

<svelte:head>
  <title>{title} | Coln Graph Lab</title>
  <meta name="description" content="The requested Coln document could not be loaded" />
</svelte:head>

<main class="grid min-h-screen min-w-80 place-items-center bg-[#101718] p-6 font-['Manrope'] text-[#e8ece8]" data-testid="document-load-error" data-error-kind={kind}>
  <section class="grid w-full max-w-xl gap-6 border border-[#304041] bg-[#182122] p-6 min-[601px]:p-10">
    <div class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.13em]">
      <span class="grid size-9 place-items-center border border-[#ff7657] text-lg text-[#ff7657]">C</span>
      <span>COLN / GRAPH LAB</span>
    </div>

    <div class="grid gap-3 border-l-3 border-[#ff7657] pl-5">
      <p class="m-0 font-['DM_Mono'] text-xs tracking-[.14em] text-[#ff9a86]">{title.toUpperCase()}</p>
      <h1 class="m-0 text-3xl font-semibold tracking-[-.03em]">{heading}</h1>
      <p class="m-0 max-w-md leading-7 text-[#aab6b6]">{detail}</p>
    </div>

    <code class="overflow-hidden text-ellipsis border border-[#304041] bg-[#101718] p-3 font-['DM_Mono'] text-xs text-[#839193]" data-testid="failed-document-url">{documentUrl}</code>

    <a class="flex h-12 items-center justify-between bg-[#d8ff57] px-4 font-bold text-[#101718] no-underline" href="/" onclick={createNewGraph}>
      Create a new graph <span class="font-['DM_Mono'] text-xl">+</span>
    </a>
  </section>
</main>
