// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { ManagedStore, type WhereClause } from "./BindingStore.js";
import {
  RowId,
  toTxnWireValue,
  type LiveTuple,
  type WireTuple,
} from "./BindingValue.js";

export interface Set<Value> {
  values(): Value[];
  contains(value: Value): boolean;
}

export interface MutableSet<Value> extends Set<Value> {
  add(): Value;
}

export interface Adaptor<Value> {
  flatten(value: Value): LiveTuple;
  reconstruct(tuple: WireTuple): Value;
}

export class BaseTableSet<Path extends string> implements MutableSet<
  RowId<Path>
> {
  constructor(
    private store: ManagedStore,
    private tableName: Path,
    private bound: LiveTuple,
  ) {}

  values(): RowId<Path>[] {
    return this.store
      .all_row_id(this.query())
      .map((value) => new RowId({ type: "Existing", value }, this.tableName));
  }

  contains(value: RowId<Path>): boolean {
    return this.store.exists({ ...this.query(), row_id: value.asWire() });
  }

  add(): RowId<Path> {
    return this.store.add(this.tableName, this.bound.map(toTxnWireValue));
  }

  private query(): WhereClause {
    return {
      table_name: this.tableName,
      row_id: null,
      values: this.bound.map((value) =>
        value instanceof RowId ? value.asWire() : value,
      ),
    };
  }
}

export class ViewTableSet<Value> implements Set<Value> {
  constructor(
    private store: ManagedStore,
    private tableName: string,
    private bound: LiveTuple,
    private select: number[],
    private adaptor: Adaptor<Value>,
  ) {}

  values(): Value[] {
    return this.store
      .all_proj(this.query(), this.select)
      .map(this.adaptor.reconstruct);
  }

  contains(value: Value): boolean {
    return this.store.exists({
      ...this.query(),
      values: [
        ...this.query().values,
        ...this.adaptor
          .flatten(value)
          .map((item) => (item instanceof RowId ? item.asWire() : item)),
      ],
    });
  }

  private query(): WhereClause {
    return {
      table_name: this.tableName,
      row_id: null,
      values: this.bound.map((value) =>
        value instanceof RowId ? value.asWire() : value,
      ),
    };
  }
}
