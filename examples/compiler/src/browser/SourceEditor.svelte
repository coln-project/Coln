<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { basicSetup } from "codemirror"
  import { Annotation, Compartment, EditorState, Transaction } from "@codemirror/state"
  import { EditorView, placeholder } from "@codemirror/view"
  import { onMount } from "svelte"

  let {
    value,
    disabled = false,
    oninput,
  }: {
    value: string
    disabled?: boolean
    oninput: (value: string) => void
  } = $props()

  let host: HTMLDivElement
  let view = $state.raw<EditorView>()
  let appliedDisabled: boolean | undefined
  const editable = new Compartment()
  const externalUpdate = Annotation.define<boolean>()

  const colnTheme = EditorView.theme({
    "&": {
      height: "100%",
      backgroundColor: "#0b1112",
      color: "#e8ece8",
      fontFamily: "'DM Mono', monospace",
      fontSize: "13px",
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
    ".cm-content": {
      caretColor: "#d8ff57",
      padding: "20px 0",
    },
    ".cm-line": {
      padding: "0 20px 0 12px",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "#d8ff57",
    },
    ".cm-selectionBackground": {
      backgroundColor: "#657a32",
    },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
      backgroundColor: "#657a32",
    },
    ".cm-content ::selection, .cm-content::selection": {
      backgroundColor: "#657a32",
      color: "#e8ece8",
    },
    ".cm-selectionMatch": {
      backgroundColor: "transparent",
      outline: "none",
    },
    ".cm-activeLine": {
      backgroundColor: "transparent",
      position: "relative",
    },
    ".cm-activeLine::before": {
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
    ".cm-foldPlaceholder": {
      backgroundColor: "#182122",
      borderColor: "#304041",
      color: "#91a0a1",
    },
    ".cm-panels": {
      backgroundColor: "#182122",
      color: "#e8ece8",
    },
    ".cm-panels.cm-panels-top": {
      borderBottom: "1px solid #304041",
    },
    ".cm-searchMatch": {
      backgroundColor: "#7f8f3f66",
      outline: "1px solid #d8ff57",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "#d8ff5744",
    },
    ".cm-tooltip": {
      backgroundColor: "#182122",
      border: "1px solid #304041",
      color: "#e8ece8",
    },
  }, { dark: true })

  function editability(disabled: boolean) {
    return [
      EditorState.readOnly.of(disabled),
      EditorView.editable.of(!disabled),
      EditorView.contentAttributes.of({ "aria-disabled": String(disabled) }),
    ]
  }

  onMount(() => {
    appliedDisabled = disabled
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        colnTheme,
        EditorView.lineWrapping,
        placeholder("Write Coln source here"),
        EditorView.contentAttributes.of({
          "aria-label": "Coln source",
          autocapitalize: "off",
          autocomplete: "off",
          spellcheck: "false",
        }),
        editable.of(editability(disabled)),
        EditorView.updateListener.of(update => {
          if (
            update.docChanged
            && !update.transactions.some(transaction => transaction.annotation(externalUpdate))
          ) {
            oninput(update.state.doc.toString())
          }
        }),
      ],
    })

    let mounted = true
    const refreshMeasurements = () => {
      if (mounted) view?.requestMeasure()
    }
    document.fonts.addEventListener("loadingdone", refreshMeasurements)
    void document.fonts.ready.then(refreshMeasurements)

    return () => {
      mounted = false
      document.fonts.removeEventListener("loadingdone", refreshMeasurements)
      view?.destroy()
    }
  })

  $effect(() => {
    if (!view) return

    const currentValue = view.state.doc.toString()
    const valueChanged = currentValue !== value
    const disabledChanged = appliedDisabled !== disabled
    if (!valueChanged && !disabledChanged) return

    appliedDisabled = disabled
    view.dispatch({
      changes: valueChanged
        ? { from: 0, to: view.state.doc.length, insert: value }
        : undefined,
      effects: disabledChanged
        ? editable.reconfigure(editability(disabled))
        : undefined,
      annotations: [
        externalUpdate.of(true),
        Transaction.addToHistory.of(false),
      ],
    })
  })
</script>

<div
  class:cursor-wait={disabled}
  class:opacity-60={disabled}
  class="min-h-[420px] w-full flex-1 overflow-hidden bg-[#0b1112] min-[761px]:min-h-0"
  bind:this={host}
></div>
