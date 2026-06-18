import { StoreHandle } from "./store";
import { Value, RowView, valueEqual, getRowRef } from "#wasm-bodge/bindings";

export type Tuple = Value[];

export namespace Tuple {
  export function equal(t0: readonly Value[], t1: readonly Value[]): boolean {
    if (t0.length == t1.length) {
      for (var i = 0; i < t0.length; i++) {
        if (!valueEqual(t0[i], t1[i])) {
          return false;
        }
      }
      return true;
    } else {
      return false;
    }
  }
}

export interface ReadonlySet {
  has(x: Value): boolean;
  values(): Iterator<RowView>;
}

export interface ReadWriteSet extends ReadonlySet {
  add(): Value;
}

export class RelTable<W> {
  path: string[];
  storeHandle: StoreHandle<W>;

  constructor(path: string[], handle: StoreHandle<W>) {
    this.path = path;
    this.storeHandle = handle;
  }

  apply_to(params: Value[]): AppliedRelTable<W> {
    return new AppliedRelTable(this, params);
  }

  storePath(): string {
    return this.path.join(".");
  }
}

export class AppliedRelTable<W> implements ReadWriteSet {
  relation: RelTable<W>;
  params: Value[];

  constructor(relation: RelTable<W>, params: Value[]) {
    this.relation = relation;
    this.params = params;
  }

  has(x: Value): boolean {
    const rowRef = getRowRef(x);
    if (rowRef === undefined) return false;

    const row = this.relation.storeHandle
      .store()
      .rowById(this.relation.storePath(), rowRef);

    return row !== undefined && Tuple.equal(row.values, this.params);
  }

  values(): Iterator<RowView> {
    const rows = this.relation.storeHandle
      .store()
      .scanTable(this.relation.storePath());

    return rows.filter((row) => Tuple.equal(row.values, this.params)).values();
  }

  add(): Value {
    const val = this.relation.storeHandle.add(
      this.relation.storePath(),
      this.params,
    );

    return val;
  }
}
