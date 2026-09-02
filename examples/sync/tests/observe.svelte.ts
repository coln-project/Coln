// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

export function observe<T>(read: () => T, update: (value: T) => void): () => void {
  return $effect.root(() => {
    $effect(() => update(read()))
  })
}
