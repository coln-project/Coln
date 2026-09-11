<!-- SPDX-FileCopyrightText: 2026 Coln contributors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

<script lang="ts">
  import { javascript } from "@codemirror/lang-javascript"
  import CodeMirrorEditor from "../lib/CodeMirrorEditor.svelte"

  let { value, disabled, onchange, onrun }: {
    value: string
    disabled: boolean
    onchange: (value: string) => void
    onrun: () => void
  } = $props()

  const extensions = [javascript()]
</script>

<CodeMirrorEditor
  {value}
  {extensions}
  {disabled}
  placeholderText="Inspect with handle.doc() or change data with handle.change(txn => …)"
  ariaLabel="Store JavaScript program"
  testId="repl-editor"
  {onchange}
  {onrun}
/>
