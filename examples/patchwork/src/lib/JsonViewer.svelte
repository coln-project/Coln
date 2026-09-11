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

<div class="coln-json json" use:renderJson={value}></div>

<style>
  .json {
    min-width: max-content;
  }
</style>
