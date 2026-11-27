// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { RowId } from "./row_id.js"
import { WhereClause, TxRowId, PublicRowId, PublicTuple, TxTuple, Path, CommitHash, Fragment } from "./types.js"

/** Read access to a consistent view of the bouncer's data. */
export interface BouncerRead {
  all_proj(query: WhereClause, select: number[]): PublicTuple[]
  all_row_id(query: WhereClause): PublicRowId[]
  one_proj(query: WhereClause, select: number[]): PublicTuple
  exists(query: WhereClause): boolean
}

/**
 * A write transaction, valid only inside the callback passed to
 * `Bouncer.write`. Reads see committed data only, not rows added in this
 * transaction.
 */
export interface BouncerWriteTxn extends BouncerRead {
  add(table_name: Path, values: TxTuple): TxRowId
}

export type CommitResult = {
  hash: CommitHash
  /** The canonical id of every row added, in the order `add` was called. */
  row_ids: PublicRowId[]
}

/**
 * The bouncer a runtime provides. Reads outside `write` each run in their own
 * implicit read transaction.
 */
export interface Bouncer extends BouncerRead {
  heads(): CommitHash[]

  /** Every commit not reachable from `haveHeads`, in dependency order. */
  fragmentsAfter(haveHeads: CommitHash[]): Fragment[]

  /**
   * Apply commits received from another replica. Each is validated and
   * applied on its own: if one is rejected, those applied before it stay
   * applied and an error is thrown. Commits whose parents are missing are
   * kept until those parents arrive.
   */
  applyChunkBytes(chunks: Uint8Array[]): void

  /**
   * Run `f` in a write transaction and commit it when `f` returns. If `f`
   * throws, the transaction is discarded and the error rethrown.
   */
  write(f: (txn: BouncerWriteTxn) => void): CommitResult
}

/** The rows added through one write transaction, and whether it is still running. */
class TxnScope {
  pending: RowId<any>[] = []
  open = true

  constructor(readonly txn: BouncerWriteTxn) {}

  finish(commit: CommitResult) {
    this.pending.forEach((id, n) => id.finish(commit.row_ids[n]))
  }

  cancel() {
    for (const id of this.pending) id.abort()
  }
}

/**
 * What generated realm objects read and write through: either the bouncer
 * itself, read-only, or a single write transaction.
 */
export class ManagedBouncer {
  /** @internal Use `Coln` rather than constructing this directly. */
  constructor(private reader: BouncerRead, private scope: TxnScope | null) {}

  private get read(): BouncerRead {
    if (this.scope !== null && !this.scope.open) {
      throw new Error("this realm belongs to a write transaction that has ended")
    }
    return this.reader
  }

  all_proj(query: WhereClause, select: number[]): PublicTuple[] {
    return this.read.all_proj(query, select)
  }

  all_row_id(query: WhereClause): PublicRowId[] {
    return this.read.all_row_id(query)
  }

  one_proj(query: WhereClause, select: [number]): PublicTuple {
    return this.read.one_proj(query, select)
  }

  // don't need one_row_id
  exists(query: WhereClause): boolean {
    return this.read.exists(query)
  }

  add<P extends string>(table_name: P, values: TxTuple): RowId<P> {
    if (this.scope === null) {
      throw new Error("cannot add outside a write transaction; use coln.write(realm => ...)")
    }
    if (!this.scope.open) {
      throw new Error("this realm belongs to a write transaction that has ended")
    }
    const id = new RowId(this.scope.txn.add(table_name, values), table_name)
    this.scope.pending.push(id)
    return id
  }
}

/** A generated realm class, as returned by `createRealm(runtime)`. */
export type RealmClass<R> = new (bouncer: ManagedBouncer) => R

function isThenable(v: unknown): boolean {
  return typeof v === "object" && v !== null && typeof (v as any).then === "function"
}

/**
 * A realm backed by a bouncer.
 *
 * ```ts
 * const db = new Coln(createRealm(runtime), bouncer)
 * const v = db.write(({ root }) => root.V.add())
 * db.realm.root.V.contains(v)
 * ```
 */
export class Coln<R> {
  /**
   * The realm for reads; each read runs in its own implicit read transaction.
   * Adding through it throws, use `write` instead.
   */
  readonly realm: R

  constructor(private Realm: RealmClass<R>, private bouncer: Bouncer) {
    this.realm = new Realm(new ManagedBouncer(bouncer, null))
  }

  /** The current heads. See `Bouncer.heads`. */
  heads(): CommitHash[] {
    return this.bouncer.heads()
  }

  /** Every commit not reachable from `haveHeads`. See `Bouncer.fragmentsAfter`. */
  fragmentsAfter(haveHeads: CommitHash[]): Fragment[] {
    return this.bouncer.fragmentsAfter(haveHeads)
  }

  /** Apply commits from another replica. See `Bouncer.applyChunkBytes`. */
  applyChunkBytes(chunks: Uint8Array[]): void {
    this.bouncer.applyChunkBytes(chunks)
  }

  /**
   * Run `f` against a realm bound to a fresh write transaction, commit it when
   * `f` returns, and return what `f` returns. Row ids added in `f` become
   * usable in queries once `write` returns. To roll back, throw from `f`: the
   * transaction is discarded and the error rethrown from `write`.
   *
   * Inside `f`, reads see committed data only, not rows added by `f`. `f` must
   * be synchronous: the transaction is committed as soon as it returns. The
   * realm passed to `f` cannot be used after `f` returns.
   */
  write<T>(f: (realm: R) => T): T {
    let scope: TxnScope | null = null
    let result!: T
    let commit: CommitResult
    try {
      commit = this.bouncer.write((txn) => {
        const s = new TxnScope(txn)
        scope = s
        try {
          result = f(new this.Realm(new ManagedBouncer(txn, s)))
        } finally {
          s.open = false
        }
        if (isThenable(result)) {
          throw new Error("write callbacks must be synchronous; the transaction has been discarded")
        }
      })
    } catch (e) {
      (scope as TxnScope | null)?.cancel()
      throw e
    }
    (scope as TxnScope | null)?.finish(commit)
    return result
  }
}
