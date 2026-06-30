// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

struct Engine<Optimizer, Runtime> {
    optimizer: Optimizer,
    runtime: Runtime,
}
