// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { mount, unmount, type Component } from "svelte"

/**
 * Bridge between Patchwork's `(handle, element) => cleanup` contract and
 * Svelte 5's mount/unmount.
 */
export function mountTool<Props extends Record<string, unknown>>(
  component: Component<Props>,
  element: HTMLElement,
  props: Props,
): () => void {
  const instance = mount(component, { target: element, props })
  return () => {
    void unmount(instance)
  }
}
