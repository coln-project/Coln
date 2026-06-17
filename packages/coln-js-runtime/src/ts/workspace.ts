import { SchemaProvider } from "./schema";
import { StorageAdapter, StoreHandle } from "./store";

export class WorkSpace {
  adapter: StorageAdapter;

  constructor(storage: StorageAdapter) {
    this.adapter = storage;
  }

  create<T>(provider: SchemaProvider<T>): StoreHandle<T> {
    return this.adapter.create(provider);
  }
}
