// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import * as native from "../../native/index.js"
import type * as types from "@coln-project/interface"

type RealmDef = {
  ir: string,
  coln_source: string,
  realm_name: string
}

type NativeRead = native.Bouncer | native.WriteTxn

class NodeRead implements types.BouncerRead {
  constructor(protected reader: NativeRead) {}

  all_proj(where: types.WhereClause, select: number[]): types.PublicTuple[] {
    return JSON.parse(this.reader.allProj(JSON.stringify(where), JSON.stringify(select)))
  }

  all_row_id(where: types.WhereClause): types.PublicRowId[] {
    return JSON.parse(this.reader.allRowId(JSON.stringify(where)))
  }

  one_proj(where: types.WhereClause, select: number[]): types.PublicTuple {
    return JSON.parse(this.reader.oneProj(JSON.stringify(where), JSON.stringify(select)))
  }

  exists(where: types.WhereClause): boolean {
    return this.reader.exists(JSON.stringify(where))
  }

  /** All rows of `table`, or `null` if there is no such table. */
  scan_table(table: types.Path): types.PublicTuple[] | null {
    const rows = this.reader.scanTable(table)
    return rows === null ? null : JSON.parse(rows)
  }
}

class NodeWriteTxn extends NodeRead implements types.BouncerWriteTxn {
  constructor(private txn: native.WriteTxn) {
    super(txn)
  }

  add(table_name: types.Path, values: types.TxTuple): types.TxRowId {
    return JSON.parse(this.txn.add(table_name, JSON.stringify(values)))
  }
}

/** The native coln-bouncer. Reads outside `write` are implicit read transactions. */
export class NodeBouncer extends NodeRead implements types.Bouncer {
  private bouncer: native.Bouncer

  constructor(def: RealmDef) {
    const bouncer = native.bouncerFromIr(def.ir, def.coln_source, def.realm_name)
    super(bouncer)
    this.bouncer = bouncer
  }

  heads(): types.CommitHash[] {
    return JSON.parse(this.bouncer.heads())
  }

  fragmentsAfter(haveHeads: types.CommitHash[]): types.Fragment[] {
    return this.bouncer.fragmentsAfter(JSON.stringify(haveHeads))
  }

  applyChunkBytes(chunks: Uint8Array[]): void {
    this.bouncer.applyChunkBytes(chunks)
  }

  write(f: (txn: types.BouncerWriteTxn) => void): types.CommitResult {
    // napi-rs does not round-trip arbitrary thrown values through native
    // code (e.g. a thrown string), so rethrow the callback's own value.
    let thrown: { value: unknown } | null = null
    let res: string
    try {
      res = this.bouncer.beginWrite((txn) => {
        try {
          f(new NodeWriteTxn(txn))
        } catch (value) {
          thrown = { value }
          throw new Error("write callback threw")
        }
      })
    } catch (e) {
      if (thrown !== null) throw (thrown as { value: unknown }).value
      throw e
    }
    return JSON.parse(res)
  }
}
