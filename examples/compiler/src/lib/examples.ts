// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

const examplesUrl = `${import.meta.env.BASE_URL}examples/`

export async function loadExampleNames(): Promise<string[]> {
  const response = await fetch(`${examplesUrl}index.json`)
  if (!response.ok) throw new Error(`Could not load examples (${response.status})`)
  return response.json() as Promise<string[]>
}

export async function loadExample(name: string): Promise<string> {
  const response = await fetch(`${examplesUrl}${encodeURIComponent(name)}`)
  if (!response.ok) throw new Error(`Could not load ${name} (${response.status})`)
  return response.text()
}
