import {
  RowRef,
  RowView,
  StoreHandle as WasmStoreHandle,
  TransactionHandle as WasmTransactionHandle,
  Value,
} from "#wasm-bodge/bindings";
import { Schema, SchemaProvider } from "./schema";


export interface StorageAdapter {
  create<W>(provider: SchemaProvider<W>): StoreHandle<W>;
}

export class ColnStoreAdapter implements StorageAdapter {
  create<W>(provider: SchemaProvider<W>): ColnStoreHandle<W> {
    return new ColnStoreHandle<W>(provider.schema());
  }
}

export interface StoreHandle<W> {
  begin(): void;
  commit(): string;

  change(db: W, cb: (db: W) => Value[]): Value[];
  add(path: string, values: Value[]): Value;

  store(): StoreView;
}

export interface StoreView {
  rowById(path: string, row_id: RowRef): RowView | undefined;
  scanTable(path: string): RowView[];
}

// A store is parameterised by W, which is the ReadWrite interface of a particular
// datbase, e.g. Graph.ReadWrite
export class ColnStoreHandle<W> implements StoreHandle<W> {
  wasmStore: WasmStoreHandle;
  tx?: WasmTransactionHandle;

  constructor(schema: Schema<W>) {
    this.wasmStore = WasmStoreHandle.fromTheory(schema.get_json());
  }

  store(): ColnStoreView {
    return new ColnStoreView(this.wasmStore);
  }

  begin(): void {
    if (this.tx) throw new Error("transaction already active");
    this.tx = this.wasmStore.beginTransaction();
  }

  commit(): string {
    if (!this.tx) throw new Error("no active transaction");
    const result = this.tx.commit();
    this.wasmStore = result.takeStore();
    this.tx = undefined;
    return result.commit;
  }

  change(db: W, cb: (db: W) => Value[]): Value[] {
    this.begin();
    const res = cb(db);

    this.commit();
    return res;
  }

  add(path: string, values: Value[]): Value {
    if (!this.tx) {
      throw new Error("no active transaction");
    }

    return this.tx.add(path, values);
  }
}

export class ColnStoreView implements StoreView {
  wasmStore: WasmStoreHandle;

  constructor(wasmStore: WasmStoreHandle) {
    this.wasmStore = wasmStore;
  }

  rowById(path: string, row_id: RowRef): RowView | undefined {
    return this.wasmStore.rowById(path, row_id);
  }

  scanTable(path: string): RowView[] {
    return this.wasmStore.scanTable(path);
  }
}
