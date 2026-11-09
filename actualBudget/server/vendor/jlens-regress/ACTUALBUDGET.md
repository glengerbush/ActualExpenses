# Vendored ECMAScript matcher

Source: https://crates.io/crates/jlens-regress/0.31.0
Archive SHA256: `af5a178d3a816cdb3f7f813b4d0eed6e260e082de4a1ad9964d0295fafdbdb1c`.
Upstream repository/revision: `.cargo_vcs_info.json`. Original MIT/Apache licenses included.

Local patch: add `Regex::find_from_ucs2_interruptible`; generalize the existing executor constructor/interruption accessor over the already-generic input/interrupt types. Add opt-in `Flags.ucs2` to stop the parser coalescing escaped surrogate pairs when compiling non-u code-unit patterns. Default UTF-8 compilation is unchanged. For the opt-in UCS2 path, count every VM instruction and precharge backreference comparison units; use the existing start-anchored search for UTF16 as well as UTF8. Poll interval and Unicode tables remain unchanged. These fixes close long-prefix near-miss work undercounting. See `actualbudget.patch`.

Default/PikeVM features disabled. Enabled: std, utf16, prohibit-unsafe. Application wrapper disables optimizer expansion and bounds pattern structure, text and deterministic matching work; native UTF-16 code units preserve JavaScript non-u semantics.
