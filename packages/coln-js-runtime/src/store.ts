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

  change<T>(db: W, cb: (db: W) => T): T;
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

  change<T>(db: W, cb: (db: W) => T): T {
    this.begin();
    const res = cb(db);

    this.commit();
    return res;
  }

  rowById(path: string, row_id: RowId): RowView | undefined {
    return this.store.rowById(path, row_id);
  }

  scanTable(path: string): RowView[] {
    return this.scanTable(path);
  }

  add(path: string, values: TxnValue[]): RowHandle {
    if (!this.tx) {
      throw new Error("no active transaction");
    }

    return this.tx.add(path, values);
  }
}
