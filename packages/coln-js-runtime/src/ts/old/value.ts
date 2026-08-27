import { Scalar } from "#wasm-bodge/bindings"

export type Value = Scalar | { [key: string]: Value } | null
