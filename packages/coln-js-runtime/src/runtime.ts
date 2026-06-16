import type { AnyValue, RowView, Value } from "./row";
import { toTxnValue, valueEqual } from "./row";
import { StoreTxnCtx } from "./store";

export type Tuple = Value[];

export namespace Tuple {
  export function equal(
    t0: readonly AnyValue[],
    t1: readonly AnyValue[],
  ): boolean {
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

export class RelTable {
  path: string[];
  ctx: StoreTxnCtx;

  constructor(path: string[], ctx: StoreTxnCtx) {
    this.path = path;
    this.ctx = ctx;
  }

  apply_to(params: Value[]): AppliedRelTable {
    return new AppliedRelTable(this, params);
  }

  storePath(): string {
    return this.path.join(".");
  }
}

export class AppliedRelTable implements ReadWriteSet {
  relation: RelTable;
  params: Value[];

  constructor(relation: RelTable, params: Value[]) {
    this.relation = relation;
    this.params = params;
  }

  has(x: Value): boolean {
    if (x.tag !== "row_id") return false;

    const rowId = x.value.tryRowId();
    if (rowId === undefined) return false;

    const row = this.relation.ctx.store.rowById(
      this.relation.storePath(),
      rowId,
    );

    return row !== undefined && Tuple.equal(row.values, this.params);
  }

  values(): Iterator<RowView> {
    const rows = this.relation.ctx.store.scanTable(this.relation.storePath());

    return rows.filter((row) => Tuple.equal(row.values, this.params)).values();
  }

  add(): Value {
    if (!this.relation.ctx.tx) {
      throw new Error("no active transaction");
    }

    const handle = this.relation.ctx.tx.add(
      this.relation.storePath(),
      this.params.map(toTxnValue),
    );

    return { tag: "row_id", value: handle };
  }
}
