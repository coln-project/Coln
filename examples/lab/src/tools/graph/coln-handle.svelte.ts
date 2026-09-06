// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import type {
  ColnChange,
  ColnDocument,
  ColnHandle as RepoColnHandle,
  RealmBindings,
} from "@coln-project/repo";
import { createSubscriber } from "svelte/reactivity";

export class ColnHandle<Bindings extends RealmBindings> {
  readonly #subscribe: () => void;

  constructor(private readonly handle: RepoColnHandle<Bindings>) {
    this.#subscribe = createSubscriber((update) => {
      handle.on("change", update);

      return () => {
        handle.off("change", update);
      };
    });
  }

  get state(): ColnDocument<Bindings> {
    this.#subscribe();
    return this.handle.doc();
  }

  change(change: ColnChange<Bindings>): void {
    this.handle.change(change);
  }
}
