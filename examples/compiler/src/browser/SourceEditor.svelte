<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let {
    value,
    disabled = false,
    oninput,
  }: {
    value: string
    disabled?: boolean
    oninput: (value: string) => void
  } = $props()
</script>

<textarea
  class="min-h-[420px] w-full flex-1 resize-y border-0 bg-[#0b1112] p-5 font-['DM_Mono'] text-[13px] leading-[1.75] text-[#e8ece8] outline-none placeholder:text-[#536163] focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-[#d8ff57] disabled:cursor-wait disabled:opacity-60 min-[761px]:min-h-0 min-[761px]:resize-none"
  {disabled}
  spellcheck="false"
  aria-label="Coln source"
  placeholder="Write Coln source here"
  {value}
  oninput={event => oninput(event.currentTarget.value)}
></textarea>
