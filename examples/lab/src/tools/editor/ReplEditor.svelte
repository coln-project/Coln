<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { javascript } from "@codemirror/lang-javascript"
  import { Compartment } from "@codemirror/state"
  import { EditorView, keymap, placeholder } from "@codemirror/view"
  import { basicSetup } from "codemirror"
  import { onMount } from "svelte"

  let {
    value,
    disabled,
    onchange,
    onrun,
  }: {
    value: string
    disabled: boolean
    onchange: (value: string) => void
    onrun: () => void
  } = $props()

  let host: HTMLDivElement
  let view: EditorView | undefined
  const editable = new Compartment()

  const colnTheme = EditorView.theme(
    {
      "&": {
        height: "100%",
        backgroundColor: "#0b1112",
        color: "#e8ece8",
        fontFamily: "'DM Mono', monospace",
        fontSize: "13px",
      },
      "&.cm-focused": { outline: "1px solid #d8ff57", outlineOffset: "-1px" },
      ".cm-scroller": { fontFamily: "inherit", lineHeight: "1.7", overflow: "auto" },
      ".cm-content": { caretColor: "#d8ff57", padding: "18px 0" },
      ".cm-line": { padding: "0 18px 0 12px" },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: "#d8ff57" },
      ".cm-selectionBackground": { backgroundColor: "#657a32" },
      "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
        backgroundColor: "#657a32",
      },
      ".cm-activeLine": { backgroundColor: "#111a1b" },
      ".cm-gutters": {
        backgroundColor: "#0b1112",
        borderRight: "1px solid #253233",
        color: "#536163",
      },
      ".cm-activeLineGutter": { backgroundColor: "#111a1b", color: "#91a0a1" },
      ".cm-tooltip, .cm-panels": {
        backgroundColor: "#182122",
        borderColor: "#304041",
        color: "#e8ece8",
      },
    },
    { dark: true },
  )

  // CodeMirror owns an imperative editable facet, so prop changes must reconfigure it.
  $effect(() => {
    view?.dispatch({ effects: editable.reconfigure(EditorView.editable.of(!disabled)) })
  })

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        javascript(),
        colnTheme,
        EditorView.lineWrapping,
        editable.of(EditorView.editable.of(!disabled)),
        placeholder("Use handle.doc() or handle.change(txn => …)"),
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              if (!disabled) onrun()
              return true
            },
          },
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onchange(update.state.doc.toString())
        }),
        EditorView.contentAttributes.of({
          "aria-label": "JavaScript program",
          autocapitalize: "off",
          autocomplete: "off",
          spellcheck: "false",
          "data-testid": "repl-editor",
        }),
      ],
    })

    const refresh = () => view?.requestMeasure()
    document.fonts.addEventListener("loadingdone", refresh)
    void document.fonts.ready.then(refresh)
    return () => {
      document.fonts.removeEventListener("loadingdone", refresh)
      view?.destroy()
      view = undefined
    }
  })
</script>

<div class="min-h-72 flex-1 overflow-hidden bg-[#0b1112]" bind:this={host}></div>
