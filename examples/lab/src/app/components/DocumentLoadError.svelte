<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let { label, heading, detail, documentUrl, action, errorKind, onaction }: {
    label: string
    heading: string
    detail: string
    documentUrl: string
    action: string
    errorKind: string
    onaction: () => void
  } = $props()
</script>

<section class="grid h-full min-h-[520px] place-items-center bg-[var(--lab-surface)] p-6" data-testid="document-load-error" data-error-kind={errorKind}>
  <div class="grid w-full max-w-xl gap-6 border border-[var(--lab-border)] bg-[var(--lab-surface-raised)] p-6 min-[601px]:p-10">
    <div class="grid gap-3 border-l-3 border-[var(--lab-error-strong)] pl-5">
      <p class="m-0 font-['DM_Mono'] text-sm tracking-[.14em] text-[var(--lab-error)]">{label}</p>
      <h1 class="m-0 text-3xl font-semibold tracking-[-.03em]">{heading}</h1>
      <p class="m-0 max-w-md leading-7 text-[#aab6b6]">{detail}</p>
    </div>
    {#if documentUrl}
      <code class="overflow-hidden text-ellipsis border border-[var(--lab-border)] bg-[var(--lab-surface)] p-3 font-['DM_Mono'] text-sm text-[#839193]" data-testid="failed-document-url">{documentUrl}</code>
    {/if}
    <button class="lab-primary-action flex h-12 items-center justify-between border-0 px-4 font-bold" type="button" onclick={onaction}>
      {action} <span class="font-['DM_Mono'] text-xl">+</span>
    </button>
  </div>
</section>
