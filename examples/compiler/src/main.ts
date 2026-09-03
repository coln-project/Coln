// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

import { mount } from "svelte"
import App from "./browser/App.svelte"
import "./style.css"

mount(App, { target: document.getElementById("app")! })
