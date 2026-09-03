<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import type { Snippet } from "svelte"

  let { label, index, children }: { label: string; index: string; children: Snippet } = $props()
</script>

<section class="min-w-0 border border-[#304041] bg-[#182122]">
  <header class="flex h-11 items-center justify-between border-b border-[#304041] px-4">
    <h2 class="m-0 font-['DM_Mono'] text-[10px] font-medium tracking-[.16em] text-[#91a0a1] uppercase">{label}</h2>
    <span class="font-['DM_Mono'] text-[10px] text-[#667576]">{index}</span>
  </header>
  <div class="min-h-16 overflow-auto p-4 font-['DM_Mono'] text-xs leading-relaxed">
    {@render children()}
  </div>
</section>
