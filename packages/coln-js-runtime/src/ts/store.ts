import {
  CommitResult,
  RowHandle,
  RowId,
  RowView,
  StoreHandle,
  TransactionHandle,
  TxnValue,
} from "#wasm-bodge/bindings";
import { Schema, SchemaProvider } from "./schema";
import { Value } from "./value";

export type { CommitResult, StoreHandle, TransactionHandle };

export interface StorageAdapter {
  create<W>(provider: SchemaProvider<W>): StorageCtx<W>;
}

export class ColnStoreAdapter implements StorageAdapter {
  create<W>(provider: SchemaProvider<W>): ColnStore<W> {
    return new ColnStore<W>(provider.schema());
  }
}

export interface StorageCtx<W> {
  begin(): void;
  commit(): string;

  change(db: W, cb: (db: W) => Value[]): Value[];
  rowById(path: string, row_id: RowId): RowView | undefined;
  scanTable(path: string): RowView[];

  add(path: string, values: TxnValue[]): RowHandle;
}

// A store is parameterised by W, which is the ReadWrite interface of a particular
// datbase, e.g. Graph.ReadWrite
export class ColnStore<W> implements StorageCtx<W> {
  store: StoreHandle;
  tx?: TransactionHandle;

  constructor(schema: Schema<W>) {
    this.store = StoreHandle.fromTheory(schema.get_json());
  }

  begin(): void {
    if (this.tx) throw new Error("transaction already active");
    this.tx = this.store.beginTransaction();
  }

  commit(): string {
    if (!this.tx) throw new Error("no active transaction");
    const result = this.tx.commit();
    this.store = result.takeStore();
    this.tx = undefined;
    return result.commit;
  }

  change(db: W, cb: (db: W) => Value[]): Value[] {
    this.begin();
    const res = cb(db);

    this.commit();
    return res.map((v) => {
      if (v.tag !== "row_handle") {
        return v;
      } else {
        return {
          tag: "row_id",
          value: v.value.rowId(),
        } as Value;
      }
    });
  }

  rowById(path: string, row_id: RowId): RowView | undefined {
    return this.store.rowById(path, row_id);
  }

  scanTable(path: string): RowView[] {
    return this.store.scanTable(path);
  }

  add(path: string, values: TxnValue[]): RowHandle {
    if (!this.tx) {
      throw new Error("no active transaction");
    }

    return this.tx.add(path, values);
  }
}
