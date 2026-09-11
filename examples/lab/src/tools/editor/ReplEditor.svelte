<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { javascript } from "@codemirror/lang-javascript"
  import { EditorView } from "@codemirror/view"
  import CodeMirrorEditor from "../../lib/CodeMirrorEditor.svelte"

  let {
    value,
    disabled,
    active = true,
    onchange,
    onrun,
  }: {
    value: string
    disabled: boolean
    active?: boolean
    onchange: (value: string) => void
    onrun: () => void
  } = $props()

  const replEditorMetrics = EditorView.theme({
    ".cm-scroller": { lineHeight: "1.7" },
    ".cm-content": { padding: "18px 0" },
    ".cm-line": { padding: "0 18px 0 12px" },
  })
  const extensions = [javascript(), replEditorMetrics]
</script>

<CodeMirrorEditor
  {value}
  {extensions}
  {disabled}
  {active}
  placeholderText="Inspect with handle.doc() or change data with handle.change(txn => …)"
  ariaLabel="Store JavaScript program"
  testId="repl-editor"
  hostClass="min-h-72 flex-1 overflow-hidden bg-[#0b1112]"
  {onchange}
  {onrun}
/>
