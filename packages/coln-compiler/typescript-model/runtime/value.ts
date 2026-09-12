import { TxnWireRowId, Value, WireRowId } from "./types.js"
import { RowId } from "./row_id.js"

export type LiveValue = Value<RowId<string>>
export type WireValue = Value<WireRowId>
export type TxnWireValue = Value<TxnWireRowId>

export type LiveTuple = LiveValue[]
export type WireTuple = WireValue[]
export type TxnWireTuple = TxnWireValue[]

export function toWire(v: LiveValue): WireValue {
  if (typeof(v) == "number") {
    return v
  } if (typeof(v) == "string") {
    return v
  } else {
    return v.asWire()
  }
}

export function toTxnWire(v: LiveValue): TxnWireValue {
  if (typeof(v) == "number") {
    return v
  } if (typeof(v) == "string") {
    return v
  } else {
    return v.asTxnWire()
  }
}
