// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Adaptor } from "./flatten.js"
import { ManagedStore } from "./store.js"
import { Path } from "./types.js"
import { LiveTuple, toWire, toTxn, toTxnWire } from "./value.js"

export interface Ref<T> {
  value(): T
}

export interface MutableRef<T> extends Ref<T> {
  set(val: T): void
}

export class BaseRef<T> implements MutableRef<T> {
  constructor(private store: ManagedStore, private table_name: Path, private bound: LiveTuple, private select: [number], private adaptor: Adaptor<T>) {}
  
  value(): T {
    return this.adaptor.reconstruct(this.store.one_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }, this.select))
  }
  
  set(val: T): void {
    this.store.add(this.table_name, [...this.bound.map(toTxnWire), ...this.adaptor.flatten(val).map(toTxnWire)])
  }
}

export class ConstRef<T> implements Ref<T> {
  constructor(private val: T) {}
  
  value(): T {
    return this.val
  }
}
