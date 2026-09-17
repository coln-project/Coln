// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { ManagedStore } from "./store.js";
import { Path, WireRowId } from "./types.js"
import { Adaptor } from "./flatten.js"
import { RowId } from "./row_id.js";
import { LiveTuple, toWire, toTxnWire } from "./value.js"

export interface Set<T> {
  values(): T[]
  contains(value: T): boolean
}

export interface MutableSet<T> extends Set<T> {
  add(): T
}

export interface Prop {
  isTrue(): boolean
}

export interface MutableProp extends Prop {
  makeTrue(): void
}

export class BaseSet<P extends string> implements MutableSet<RowId<P>> {
  constructor(private store: ManagedStore, private table_name: P, private bound: LiveTuple) {}
  
  values(): RowId<P>[] {
    return this.store.all_row_id({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }).map((i: WireRowId) => {
      return new RowId({ type: "Existing", value: i }, this.table_name)
    })
  }
  
  contains(v: RowId<P>): boolean {
    return this.store.exists({
      table_name: this.table_name,
      row_id: v.asWire(),
      values: this.bound.map(toWire)
    })
  }
  
  add(): RowId<P> {
    return this.store.add(this.table_name, this.bound.map(toTxnWire))
  }
}

export class BaseProp implements MutableProp {
  constructor(private store: ManagedStore, private table_name: string, private bound: LiveTuple) {}
  
  isTrue(): boolean {
    return this.store.exists({table_name: this.table_name, row_id: null, values: this.bound.map(toWire)})
  }
  
  makeTrue(): void {
    this.store.add(this.table_name, this.bound.map(toTxnWire))
  }
}

export class ConjunctiveViewSet<T> implements Set<T> {
  constructor(private store: ManagedStore, private table_name: Path, private bound: LiveTuple, private select: number[], private adaptor: Adaptor<T>) {}

  values(): T[] {
    return this.store.all_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toWire)
    }, this.select).map(this.adaptor.reconstruct)
  }
  
  contains(v: T): boolean {
    return this.store.exists({
      table_name: this.table_name,
      row_id: null,
      values: [...this.bound.map(toWire), ...this.adaptor.flatten(v)]
    })
  }
}

export class ViewProp implements Prop {
  constructor(private store: ManagedStore, private table_name: string, private bound: LiveTuple) {}

  isTrue(): boolean {
    return this.store.exists({table_name: this.table_name, row_id: null, values: this.bound.map(toWire)})
  }
}
