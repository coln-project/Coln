// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { Adaptor } from "./flatten.js"
import { ManagedBouncer } from "./bouncer.js"
import { Path } from "./types.js"
import { LiveTuple, toPublic, publicToTx, toTx } from "./value.js"

export interface Ref<T> {
  value(): T
}

export interface MutableRef<T> extends Ref<T> {
  set(val: T): void
}

export class BaseRef<T> implements MutableRef<T> {
  constructor(private bouncer: ManagedBouncer, private table_name: Path, private bound: LiveTuple, private select: [number], private adaptor: Adaptor<T>) {}
  
  value(): T {
    return this.adaptor.reconstruct(this.bouncer.one_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toPublic)
    }, this.select))
  }
  
  set(val: T): void {
    this.bouncer.add(this.table_name, [...this.bound.map(toTx), ...this.adaptor.flatten(val).map(toTx)])
  }
}

export class ConstRef<T> implements Ref<T> {
  constructor(private val: T) {}
  
  value(): T {
    return this.val
  }
}
