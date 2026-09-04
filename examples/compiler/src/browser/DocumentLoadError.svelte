<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let { documentUrl, kind }: {
    documentUrl: string
    kind: "invalid" | "unavailable" | "incompatible"
  } = $props()

  const title = $derived(
    kind === "invalid"
      ? "Document URL invalid"
      : kind === "incompatible"
        ? "Document incompatible"
        : "Document unavailable",
  )
  const heading = $derived(
    kind === "invalid"
      ? "This is not a valid Automerge document URL."
      : kind === "incompatible"
        ? "This document is not a supported Coln theory."
        : "This theory could not be found.",
  )
  const detail = $derived(
    kind === "invalid"
      ? "Check the URL and try again, or create a new theory."
      : kind === "incompatible"
        ? "Its structure or version does not match this compiler."
        : "It may not have synchronized yet, or may no longer be available.",
  )

  function createNewTheory(event: MouseEvent) {
    event.preventDefault()
    history.replaceState(null, "", `${location.pathname}${location.search}`)
    location.reload()
  }
</script>

<svelte:head>
  <title>{title} | Coln Compiler</title>
</svelte:head>

<main class="grid min-h-screen min-w-80 place-items-center bg-[#101718] p-6 font-['Manrope'] text-[#e8ece8]">
  <section class="grid w-full max-w-xl gap-6 border border-[#304041] bg-[#182122] p-6 min-[601px]:p-10">
    <div class="flex items-center gap-3 font-['DM_Mono'] text-xs font-medium tracking-[.13em]">
      <span class="grid size-9 place-items-center border border-[#ff7657] text-lg text-[#ff7657]">C</span>
      <span>COLN / COMPILER</span>
    </div>

    <div class="grid gap-3 border-l-3 border-[#ff7657] pl-5">
      <p class="m-0 font-['DM_Mono'] text-xs tracking-[.14em] text-[#ff9a86]">{title.toUpperCase()}</p>
      <h1 class="m-0 text-3xl font-semibold tracking-[-.03em]">{heading}</h1>
      <p class="m-0 max-w-md leading-7 text-[#aab6b6]">{detail}</p>
    </div>

    <code class="overflow-hidden text-ellipsis border border-[#304041] bg-[#101718] p-3 font-['DM_Mono'] text-xs text-[#839193]">{documentUrl}</code>

    <a class="flex h-12 items-center justify-between bg-[#d8ff57] px-4 font-bold text-[#101718] no-underline" href="/" onclick={createNewTheory}>
      Create a new theory <span class="font-['DM_Mono'] text-xl">+</span>
    </a>
  </section>
</main>
