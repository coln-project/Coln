<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  let { message, kind = "success" }: {
    message: string
    kind?: "success" | "error"
  } = $props()
</script>

<p
  class="notice"
  data-kind={kind}
  role={kind === "error" ? "alert" : "status"}
>{message}</p>

<style>
  .notice {
    background: var(--coln-surface-raised);
    border: 1px solid var(--coln-accent);
    border-radius: var(--studio-radius-sm, 4px);
    box-shadow: var(--studio-shadow-md, 0 8px 24px rgb(0 0 0 / 0.2));
    color: var(--coln-accent-text);
    font-family: var(--coln-mono);
    font-size: 0.875rem;
    left: 50%;
    margin: 0;
    max-width: calc(100% - 2rem);
    padding: 0.5rem 1rem;
    position: absolute;
    top: 0.75rem;
    transform: translateX(-50%);
    z-index: 50;
  }

  .notice[data-kind="error"] {
    background: var(--coln-alert-fill);
    border-color: var(--coln-danger);
    color: var(--coln-danger-text);
  }
</style>
