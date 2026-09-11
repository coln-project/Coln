// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

const prefix = "coln-patchwork:repl-source:"

export const starterSource = `const doc = handle.doc()
console.log(JSON.parse(doc.jsonIR()))
return doc.heads()`

export function loadSource(documentUrl: string): string {
  try {
    return localStorage.getItem(prefix + documentUrl) ?? starterSource
  } catch {
    return starterSource
  }
}

export function saveSource(documentUrl: string, source: string): void {
  try {
    localStorage.setItem(prefix + documentUrl, source)
  } catch {
    // The REPL remains usable when storage is unavailable.
  }
}
