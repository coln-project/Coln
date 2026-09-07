<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { Compartment, type Extension } from "@codemirror/state"
  import { EditorView, keymap, placeholder } from "@codemirror/view"
  import { basicSetup } from "codemirror"
  import { onMount } from "svelte"
  import { labEditorTheme } from "./codemirror-theme.ts"

  let {
    value,
    extensions,
    disabled = false,
    active = true,
    placeholderText,
    ariaLabel,
    testId,
    hostClass,
    onchange,
    onrun,
  }: {
    value: string
    extensions?: Extension
    disabled?: boolean
    active?: boolean
    placeholderText: string
    ariaLabel: string
    testId?: string
    hostClass: string
    onchange?: (value: string) => void
    onrun?: () => void
  } = $props()

  let host: HTMLDivElement
  let view: EditorView | undefined
  const editable = new Compartment()

  $effect(() => {
    view?.dispatch({ effects: editable.reconfigure(EditorView.editable.of(!disabled)) })
  })

  $effect(() => {
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
    }
  })

  $effect(() => {
    if (active) requestAnimationFrame(() => view?.requestMeasure())
  })

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        ...(extensions ? [extensions] : []),
        labEditorTheme,
        EditorView.lineWrapping,
        editable.of(EditorView.editable.of(!disabled)),
        placeholder(placeholderText),
        ...(onrun ? [keymap.of([{
          key: "Ctrl-Enter",
          run: () => {
            if (!disabled) onrun()
            return true
          },
        }])] : []),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onchange?.(update.state.doc.toString())
        }),
        EditorView.contentAttributes.of({
          "aria-label": ariaLabel,
          autocapitalize: "off",
          autocomplete: "off",
          spellcheck: "false",
          ...(testId ? { "data-testid": testId } : {}),
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

<div class={hostClass} bind:this={host}></div>
