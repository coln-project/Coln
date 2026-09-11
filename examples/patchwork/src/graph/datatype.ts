// SPDX-FileCopyrightText: 2026 Coln contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { pointerDatatype } from "../coln/pointer.ts"

export const GRAPH_TYPE = "coln-graph"

/**
 * A Patchwork document pointing at a Coln store that uses the checked-in
 * `GraphRealm` schema (src/graph/graph.coln). The Graph Demo refuses stores
 * with any other schema, since its generated bindings would not match.
 */
export const ColnGraphDatatype = pointerDatatype(GRAPH_TYPE, "Coln graph")

export default ColnGraphDatatype
