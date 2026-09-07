// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { EditorView } from "@codemirror/view"

export const labEditorTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      backgroundColor: "#0b1112",
      color: "#e8ece8",
      fontFamily: "'DM Mono', monospace",
      fontSize: "14px",
    },
    "&.cm-focused": {
      outline: "1px solid #d8ff57",
      outlineOffset: "-1px",
    },
    ".cm-scroller": {
      fontFamily: "inherit",
      lineHeight: "1.75",
      overflow: "auto",
    },
    ".cm-content": { caretColor: "#d8ff57", padding: "20px 0" },
    ".cm-line": { padding: "0 20px 0 12px" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#d8ff57" },
    ".cm-selectionBackground": { backgroundColor: "#657a32" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
      backgroundColor: "#657a32",
    },
    ".cm-content ::selection, .cm-content::selection": {
      backgroundColor: "#657a32",
      color: "#e8ece8",
    },
    ".cm-selectionMatch": { backgroundColor: "transparent", outline: "none" },
    ".cm-content .cm-activeLine": {
      backgroundColor: "transparent",
      position: "relative",
    },
    ".cm-content .cm-activeLine::before": {
      backgroundColor: "#111a1b",
      content: "''",
      inset: "0",
      pointerEvents: "none",
      position: "absolute",
      zIndex: "-3",
    },
    ".cm-gutters": {
      backgroundColor: "#0b1112",
      borderRight: "1px solid #253233",
      color: "#536163",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "#111a1b",
      color: "#91a0a1",
    },
    ".cm-foldPlaceholder, .cm-tooltip, .cm-panels": {
      backgroundColor: "#182122",
      borderColor: "#304041",
      color: "#e8ece8",
    },
    ".cm-panels.cm-panels-top": { borderBottom: "1px solid #304041" },
    ".cm-searchMatch": {
      backgroundColor: "#7f8f3f66",
      outline: "1px solid #d8ff57",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "#d8ff5744",
    },
    ".cm-tooltip": { border: "1px solid #304041" },
  },
  { dark: true },
)
