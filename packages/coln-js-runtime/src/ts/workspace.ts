import { SchemaProvider } from "./schema";
import { StorageAdapter, StorageCtx } from "./store";


export class WorkSpace {
  adapter: StorageAdapter;

  constructor(storage: StorageAdapter) {
    this.adapter = storage;
  }

  create<T>(provider: SchemaProvider<T>): StorageCtx<T> {
    return this.adapter.create(provider);
  }
}
