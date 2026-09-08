// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { EditorView } from "@codemirror/view"

/**
 * CodeMirror styling for both editors here. Every colour reads a `--coln-*`
 * variable (see lib/theme.css) rather than a literal, so the editor follows
 * the Patchwork document theme like the rest of the tool.
 */
export const colnEditorTheme = EditorView.theme({
  "&": {
    backgroundColor: "var(--coln-fill)",
    color: "var(--coln-line)",
    fontFamily: "var(--coln-mono)",
    fontSize: "0.875rem",
    height: "100%",
  },
  "&.cm-focused": {
    outline: "1px solid var(--coln-accent)",
    outlineOffset: "-1px",
  },
  ".cm-scroller": {
    fontFamily: "inherit",
    lineHeight: "1.7",
    overflow: "auto",
  },
  ".cm-content": {
    caretColor: "var(--coln-accent)",
    padding: "1rem 0",
  },
  ".cm-line": { padding: "0 1rem 0 0.75rem" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--coln-accent)" },
  ".cm-selectionBackground, ::selection": {
    backgroundColor: "var(--coln-selected-fill)",
  },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    backgroundColor: "var(--coln-selected-fill)",
  },
  ".cm-selectionMatch": { backgroundColor: "transparent", outline: "none" },
  ".cm-activeLine": { backgroundColor: "var(--coln-surface)" },
  ".cm-gutters": {
    backgroundColor: "var(--coln-fill)",
    borderRight: "1px solid var(--coln-border)",
    color: "var(--coln-faint)",
  },
  ".cm-activeLineGutter": {
    backgroundColor: "var(--coln-surface)",
    color: "var(--coln-muted)",
  },
  ".cm-foldPlaceholder, .cm-tooltip, .cm-panels": {
    backgroundColor: "var(--coln-surface-raised)",
    borderColor: "var(--coln-border)",
    color: "var(--coln-line)",
  },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--coln-border)" },
  ".cm-searchMatch": {
    backgroundColor: "var(--coln-selected-fill)",
    outline: "1px solid var(--coln-accent)",
  },
  ".cm-tooltip": { border: "1px solid var(--coln-border)" },
})
