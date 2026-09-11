<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { Compartment, type Extension } from "@codemirror/state"
  import { EditorView, keymap, placeholder } from "@codemirror/view"
  import { basicSetup } from "codemirror"
  import { onMount } from "svelte"
  import { colnEditorTheme } from "./codemirror-theme.ts"

  let {
    value,
    extensions,
    disabled = false,
    placeholderText,
    ariaLabel,
    testId,
    onchange,
    onrun,
  }: {
    value: string
    extensions?: Extension
    disabled?: boolean
    placeholderText: string
    ariaLabel: string
    testId?: string
    onchange?: (value: string) => void
    onrun?: () => void
  } = $props()

  let host: HTMLDivElement
  let view: EditorView | undefined
  const editable = new Compartment()

  $effect(() => {
    view?.dispatch({
      effects: editable.reconfigure(EditorView.editable.of(!disabled)),
    })
  })

  $effect(() => {
    if (view && value !== view.state.doc.toString()) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value },
      })
    }
  })

  onMount(() => {
    view = new EditorView({
      doc: value,
      parent: host,
      extensions: [
        basicSetup,
        ...(extensions ? [extensions] : []),
        colnEditorTheme,
        EditorView.lineWrapping,
        editable.of(EditorView.editable.of(!disabled)),
        placeholder(placeholderText),
        ...(onrun
          ? [
              keymap.of([
                {
                  key: "Ctrl-Enter",
                  run: () => {
                    if (!disabled) onrun()
                    return true
                  },
                },
              ]),
            ]
          : []),
        EditorView.updateListener.of(update => {
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

    return () => {
      view?.destroy()
      view = undefined
    }
  })
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    background: var(--coln-fill);
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
</style>
