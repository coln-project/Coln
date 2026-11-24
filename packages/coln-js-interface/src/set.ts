// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { ManagedBouncer } from "./bouncer.js";
import { Path, PublicRowId } from "./types.js"
import { Adaptor } from "./flatten.js"
import { RowId } from "./row_id.js";
import { LiveTuple, toPublic, toTx } from "./value.js"

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
  constructor(private bouncer: ManagedBouncer, private table_name: P, private bound: LiveTuple) {}
  
  values(): RowId<P>[] {
    return this.bouncer.all_row_id({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toPublic)
    }).map((i: PublicRowId) => {
      return new RowId({ type: "Existing", value: i }, this.table_name)
    })
  }
  
  contains(v: RowId<P>): boolean {
    return this.bouncer.exists({
      table_name: this.table_name,
      row_id: v.asPublic(),
      values: this.bound.map(toPublic)
    })
  }
  
  add(): RowId<P> {
    return this.bouncer.add(this.table_name, this.bound.map(toTx))
  }
}

export class BaseProp implements MutableProp {
  constructor(private bouncer: ManagedBouncer, private table_name: string, private bound: LiveTuple) {}
  
  isTrue(): boolean {
    return this.bouncer.exists({table_name: this.table_name, row_id: null, values: this.bound.map(toPublic)})
  }
  
  makeTrue(): void {
    this.bouncer.add(this.table_name, this.bound.map(toTx))
  }
}

export class ConjunctiveViewSet<T> implements Set<T> {
  constructor(private bouncer: ManagedBouncer, private table_name: Path, private bound: LiveTuple, private select: number[], private adaptor: Adaptor<T>) {}

  values(): T[] {
    return this.bouncer.all_proj({
      table_name: this.table_name,
      row_id: null,
      values: this.bound.map(toPublic)
    }, this.select).map(this.adaptor.reconstruct)
  }
  
  contains(v: T): boolean {
    return this.bouncer.exists({
      table_name: this.table_name,
      row_id: null,
      values: [...this.bound.map(toPublic), ...this.adaptor.flatten(v).map(toPublic)]
    })
  }
}

export class ViewProp implements Prop {
  constructor(private bouncer: ManagedBouncer, private table_name: string, private bound: LiveTuple) {}

  isTrue(): boolean {
    return this.bouncer.exists({table_name: this.table_name, row_id: null, values: this.bound.map(toPublic)})
  }
}
