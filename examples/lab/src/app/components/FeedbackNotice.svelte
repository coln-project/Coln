<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let { message, kind = "success" }: {
    message: string
    kind?: "success" | "error"
  } = $props()
</script>

<p class="feedback-notice fixed top-3 left-1/2 z-50 m-0 max-w-[calc(100%-2rem)] -translate-x-1/2 px-4 py-2 font-['DM_Mono'] text-sm shadow-[0_8px_24px_#0008] min-[761px]:top-4" class:error={kind === "error"} role={kind === "error" ? "alert" : "status"}>{message}</p>

<style>
  .feedback-notice {
    background: var(--lab-surface-raised);
    border: 1px solid var(--lab-accent);
    color: var(--lab-accent);
    font-weight: 500;
  }
  .feedback-notice.error {
    background: #211918;
    border-color: var(--lab-error-strong);
    color: var(--lab-error);
    font-weight: 400;
  }
</style>
