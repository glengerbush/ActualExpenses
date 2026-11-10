# Trusted formula assets

HyperFormula3.5.0 comes from the dependency installed by pinned Actual API26.10.0
and is used under `gpl-v3`, matching the source application. Original bundle and
license are retained. The enUS dictionary is serialized without changing values.
`custom-functions.js` bundles unchanged pinned Actual custom functions, currencies
and their used utility dependencies; no formula parser/calculator is substituted.

Reproduce/check with `node actualBudget/tools/generate-formula-runtime.mjs --check`
from repository root (existing compatibility/web dependencies required). The
manifest records every used Actual source hash and each asset digest. Building
or serving the Rust application requires neither Node nor these dependency trees.
The runtime only loads these embedded assets; it never loads caller bytecode or
JavaScript and provides no filesystem/network/process/module-loader host APIs.

QuickJS/rquickjs use their upstream MIT licensing; ICU4X uses Unicode-3.0.
Their exact sources/versions/checksums are retained in Cargo.lock. ICU collation
supplies the locale/strength/case/punctuation behavior required by HyperFormula;
there is no lexical-comparison fallback. Locale data versions can differ across
source browser/Node platforms; representative Unicode behavior is source-tested.
