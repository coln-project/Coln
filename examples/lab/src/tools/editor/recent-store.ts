// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

const key = "coln-store-editor:recent-store"

export function loadRecentStore(): string {
  try {
    return localStorage.getItem(key) ?? ""
  } catch {
    return ""
  }
}

export function saveRecentStore(documentUrl: string): void {
  try {
    localStorage.setItem(key, documentUrl)
  } catch {
    // Store loading remains usable when storage is unavailable.
  }
}
