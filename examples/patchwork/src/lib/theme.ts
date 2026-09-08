// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import css from "./theme.css?inline"

/**
 * Add the shared Coln stylesheet to the page, once.
 *
 * Patchwork tools render into the light DOM, so this is a plain `<style>` in
 * the head rather than anything scoped — every rule inside is namespaced under
 * `.coln-tool` (or `.coln-json` / `.coln-diagnostics`) to stay out of the rest
 * of Patchwork. It deliberately outlives an unmount: all three tools share it,
 * and removing it when one closes would strip another one's styling.
 *
 * Component-level CSS is not here; vite-plugin-svelte is configured with
 * `emitCss: false`, so each component's `<style>` block travels inside its own
 * JS and is scoped by the Svelte compiler.
 */
export function injectTheme(): void {
  if (document.querySelector("style[data-coln-theme]")) return
  const style = document.createElement("style")
  style.dataset.colnTheme = ""
  style.textContent = css
  document.head.append(style)
}
