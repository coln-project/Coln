<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import JSONFormatter from "json-formatter-js"

  let { value }: { value: string } = $props()

  function renderJson(node: HTMLElement, initialValue: string) {
    function render(nextValue: string) {
      try {
        const formatter = new JSONFormatter(JSON.parse(nextValue), 2, {
          animateClose: false,
          animateOpen: false,
          hoverPreviewEnabled: true,
          theme: "dark",
        })
        node.replaceChildren(formatter.render())
      } catch {
        const fallback = document.createElement("pre")
        fallback.className = "json-fallback"
        fallback.textContent = nextValue
        node.replaceChildren(fallback)
      }
    }

    render(initialValue)
    return { update: render }
  }
</script>

<div class="json-output" use:renderJson={value}></div>

<style>
  .json-output {
    min-width: max-content;
  }

  :global(.json-output .json-formatter-row) {
    font-family: "DM Mono", monospace;
    line-height: 1.65;
  }

  :global(.json-output .json-formatter-row),
  :global(.json-output .json-formatter-row a),
  :global(.json-output .json-formatter-row a:hover) {
    color: #91a0a1;
  }

  :global(.json-output .json-formatter-key) {
    color: #77b7ff;
  }

  :global(.json-output .json-formatter-string),
  :global(.json-output .json-formatter-stringifiable) {
    color: #d8ff57;
  }

  :global(.json-output .json-formatter-number) {
    color: #ffd166;
  }

  :global(.json-output .json-formatter-boolean) {
    color: #ff9a86;
  }

  :global(.json-output .json-formatter-null) {
    color: #bba4ff;
  }

  :global(.json-output .json-formatter-bracket) {
    color: #667576;
  }

  :global(.json-output .json-formatter-toggler) {
    color: #d8ff57;
  }

  :global(.json-fallback) {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
