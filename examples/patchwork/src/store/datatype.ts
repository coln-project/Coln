// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { pointerDatatype } from "../coln/pointer.ts"

export const STORE_TYPE = "coln-store"

/**
 * A Patchwork document pointing at one Coln store, browsed by the Store
 * Editor. See coln/pointer.ts for why the pointer exists.
 */
export const ColnStoreDatatype = pointerDatatype(STORE_TYPE, "Coln store")

export default ColnStoreDatatype
