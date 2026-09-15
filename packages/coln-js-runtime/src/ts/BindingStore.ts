// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import {
  abortRowId,
  finishRowId,
  RowId,
  type CommitHash,
  type TxnWireRowId,
  type TxnWireTuple,
  type WireRowId,
  type WireTuple,
  type WireValue,
} from "./BindingValue.js";

export type WhereClause = {
  // Logical relation name. Backends decide how the relation is materialized.
  table_name: string;
  row_id: WireRowId | null;
  values: WireValue[];
};

export interface ManagedStoreBackend<Store> {
  all_proj(query: WhereClause, select: number[]): WireTuple[];
  all_row_id(query: WhereClause): WireRowId[];
  one_proj(query: WhereClause, select: number[]): WireTuple;
  exists(query: WhereClause): boolean;
  add(path: string, values: TxnWireTuple): TxnWireRowId;
  commit(): { commit: CommitHash; store: Store };
  abort(): Store;
}

export class ManagedStore<Store = unknown> {
  #pending: RowId<string>[] = [];

  constructor(private backend: ManagedStoreBackend<Store>) {}

  all_proj(query: WhereClause, select: number[]): WireTuple[] {
    return this.backend.all_proj(query, select);
  }

  all_row_id(query: WhereClause): WireRowId[] {
    return this.backend.all_row_id(query);
  }

  one_proj(query: WhereClause, select: number[]): WireTuple {
    return this.backend.one_proj(query, select);
  }

  exists(query: WhereClause): boolean {
    return this.backend.exists(query);
  }

  add<Path extends string>(tableName: Path, values: TxnWireTuple): RowId<Path> {
    const rowId = new RowId(this.backend.add(tableName, values), tableName);
    this.#pending.push(rowId);
    return rowId;
  }

  commit(): Store {
    const { commit, store } = this.backend.commit();
    for (const rowId of this.#pending) finishRowId(rowId, commit);
    this.#pending = [];
    return store;
  }

  abort(): Store {
    const store = this.backend.abort();
    for (const rowId of this.#pending) abortRowId(rowId);
    this.#pending = [];
    return store;
  }
}
