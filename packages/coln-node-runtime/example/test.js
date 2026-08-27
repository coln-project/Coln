import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const coln = require("../coln-bindings.node");

coln.hello()
