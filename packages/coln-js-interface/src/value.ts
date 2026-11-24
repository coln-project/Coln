// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { TxRowId, PublicRowId, PublicScalar } from "./types.js"
import { RowId } from "./row_id.js"

/**
 * A scalar: a row id, a plain JS number or a string. The runtimes send and
 * receive scalars in this same untagged form (see `PublicScalar`).
 */
export type Value<I> = PublicScalar<I>

export type LiveValue = Value<RowId<string>>
export type PublicScalarValue = Value<PublicRowId>
export type TxScalarValue = Value<TxRowId>

export type LiveTuple = LiveValue[]

const I32_MIN = -(2 ** 31)
const I32_MAX = 2 ** 31 - 1

/**
 * Coln's only integer type, `Int`, is stored as `i32`. The runtime rejects
 * anything else too, but checking here gives a clearer error.
 */
function checkI32(n: number): number {
  if (!Number.isInteger(n) || n < I32_MIN || n > I32_MAX) {
    throw new RangeError(`${n} is not a 32-bit integer`)
  }
  return n
}

function resolve<I>(v: LiveValue, rowId: (id: RowId<string>) => I): Value<I> {
  if (typeof v == "number") {
    return checkI32(v)
  } else if (typeof v == "string") {
    return v
  } else {
    return rowId(v)
  }
}

export function toPublic(v: LiveValue): PublicScalarValue {
  return resolve(v, (id) => id.asPublic())
}

export function toTx(v: LiveValue): TxScalarValue {
  return resolve(v, (id) => id.asTx())
}

export function publicToTx(v: PublicScalarValue): TxScalarValue {
  if (typeof(v) == "number") {
    return v
  } if (typeof(v) == "string") {
    return v
  } else {
    return { "type": "Existing", "value": v }
  }
}
