//! Bounded ECMAScript non-u matching for register filters, LIKE and tags.
//! Pattern and input use UTF-16 code units, including surrogate halves. Never
//! translate regex syntax: the engine owns ECMAScript parsing and semantics.
use crate::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

const PATTERN_UNITS: usize = 2048;
const STRUCTURAL_TOKENS: usize = 256;
const NESTING: usize = 64;
const TEXT_UNITS: usize = 20000;
const MATCH_POLLS: u64 = 10;
const REQUEST_WORK: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Failure {
    Invalid,
    Complexity,
    Budget,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "Invalid ECMAScript regular expression",
            Self::Complexity => "Regular expression exceeds supported compilation limits; simplify the pattern",
            Self::Budget => "Register pattern work limit reached; narrow the account/date range or simplify the pattern and retry",
        })
    }
}
impl std::error::Error for Failure {}
impl Failure {
    pub(crate) fn api_error(&self) -> Error {
        Error::new(
            axum::http::StatusCode::BAD_REQUEST,
            match self {
                Self::Invalid => "invalid_regex",
                Self::Complexity => "regex_complexity_limit",
                Self::Budget => "regex_work_limit",
            },
            self.to_string(),
        )
    }
}

pub(crate) struct Pattern {
    regex: regress::Regex,
    units: u64,
}
impl Pattern {
    pub(crate) fn compile(source: &str, insensitive: bool) -> Result<Self, Failure> {
        let units: Vec<u16> = source.encode_utf16().take(PATTERN_UNITS + 1).collect();
        if units.len() > PATTERN_UNITS {
            return Err(Failure::Complexity);
        }
        // Bound parser/AST depth and size before invoking the engine. Escaped
        // punctuation and class contents do not represent nesting operators.
        let mut depth = 0usize;
        let mut tokens = 0usize;
        let mut escaped = false;
        let mut class = false;
        for &unit in &units {
            if escaped {
                escaped = false;
                continue;
            }
            if unit == b'\\' as u16 {
                escaped = true;
                continue;
            }
            if class {
                if unit == b']' as u16 {
                    class = false;
                }
                continue;
            }
            match unit {
                n if n == b'[' as u16 => {
                    class = true;
                    tokens += 1;
                }
                n if n == b'(' as u16 => {
                    depth += 1;
                    tokens += 1;
                }
                n if n == b')' as u16 => depth = depth.saturating_sub(1),
                n if b"{|*+?".iter().any(|&c| u16::from(c) == n) => tokens += 1,
                _ => {}
            }
            if depth > NESTING || tokens > STRUCTURAL_TOKENS {
                return Err(Failure::Complexity);
            }
        }
        let regex = regress::Regex::from_unicode(
            units.iter().copied().map(u32::from),
            regress::Flags {
                icase: insensitive,
                ucs2: true,
                no_opt: true, // No loop unrolling/AST expansion during compilation.
                ..Default::default()
            },
        )
        .map_err(|_| Failure::Invalid)?;
        Ok(Self {
            regex,
            units: units.len() as u64,
        })
    }
    pub(crate) fn matches(&self, text: &str, budget: &Budget) -> Result<bool, Failure> {
        let text: Vec<u16> = text.encode_utf16().take(TEXT_UNITS + 1).collect();
        if text.len() > TEXT_UNITS {
            return Err(Failure::Budget);
        }
        // Precharge every invocation, including matches too cheap to poll. The
        // quantum covers the unobserved tail, plus pattern/input traversal.
        budget.charge(regress::INTERRUPT_INTERVAL as u64 + self.units + text.len() as u64)?;
        let mut polls = 0;
        self.regex
            .find_from_ucs2_interruptible(&text, 0, || {
                polls += 1;
                polls >= MATCH_POLLS || budget.charge(regress::INTERRUPT_INTERVAL as u64).is_err()
            })
            .map(|found| found.is_some())
            .map_err(|_| Failure::Budget)
    }
}
#[derive(Clone)]
pub(crate) struct Budget {
    remaining: Arc<AtomicU64>,
    failure: Arc<AtomicU8>,
}
impl Budget {
    fn failed(&self, failure: Failure) -> Failure {
        self.failure.store(
            match failure {
                Failure::Invalid => 1,
                Failure::Complexity => 2,
                Failure::Budget => 3,
            },
            Ordering::Relaxed,
        );
        failure
    }
    pub(crate) fn error(&self) -> Option<Error> {
        match self.failure.load(Ordering::Relaxed) {
            1 => Some(Failure::Invalid.api_error()),
            2 => Some(Failure::Complexity.api_error()),
            3 => Some(Failure::Budget.api_error()),
            _ => None,
        }
    }
    fn charge(&self, amount: u64) -> Result<(), Failure> {
        let mut left = self.remaining.load(Ordering::Relaxed);
        loop {
            let next = left.checked_sub(amount).ok_or(Failure::Budget)?;
            match self.remaining.compare_exchange_weak(
                left,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(current) => left = current,
            }
        }
    }
}
/// Install one request budget shared by every pattern, predicate and SQL pass.
pub(crate) fn install(db: &rusqlite::Connection) -> Result<Budget, Error> {
    use rusqlite::functions::FunctionFlags;
    let budget = Budget {
        remaining: Arc::new(AtomicU64::new(REQUEST_WORK)),
        failure: Arc::new(AtomicU8::new(0)),
    };
    for (name, insensitive) in [("register_regex", false), ("register_like", true)] {
        let budget = budget.clone();
        // Not marked deterministic: work charging must happen for each call.
        db.create_scalar_function(name, 2, FunctionFlags::SQLITE_UTF8, move |ctx| {
            type BoxError = Box<dyn std::error::Error + Send + Sync>;
            let pattern = ctx.get_or_create_aux(0, |v| -> Result<_, BoxError> {
                Ok(Pattern::compile(v.as_str()?, insensitive).map_err(|e| budget.failed(e))?)
            })?;
            pattern
                .matches(&ctx.get::<String>(1)?, &budget)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(budget.failed(e))))
        })?;
    }
    Ok(budget)
}
/// Escape only ECMAScript syntax characters; this is also used by tag/LIKE construction.
pub(crate) fn escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if "^$\\.*+?()[]{}|".contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn replacement_budget() -> Budget {
    Budget {
        remaining: Arc::new(AtomicU64::new(REQUEST_WORK)),
        failure: Arc::new(AtomicU8::new(0)),
    }
}
impl Pattern {
    /// ECMAScript String.replaceAll substitution over bounded non-u UTF-16 matches.
    pub(crate) fn replace_all(
        &self,
        text: &str,
        replacement: &str,
        budget: &Budget,
    ) -> Result<String, Error> {
        let text: Vec<u16> = text.encode_utf16().collect();
        let replacement: Vec<u16> = replacement.encode_utf16().collect();
        if text.len() > TEXT_UNITS || replacement.len() > TEXT_UNITS {
            return Err(Failure::Budget.api_error());
        }
        let mut output = Vec::new();
        let mut position = 0;
        let mut last = 0;
        while position <= text.len() {
            budget
                .charge(regress::INTERRUPT_INTERVAL as u64 + self.units + text.len() as u64)
                .map_err(|e| e.api_error())?;
            let mut polls = 0;
            let found = self
                .regex
                .find_from_ucs2_interruptible(&text, position, || {
                    polls += 1;
                    polls >= MATCH_POLLS
                        || budget.charge(regress::INTERRUPT_INTERVAL as u64).is_err()
                })
                .map_err(|_| Failure::Budget.api_error())?;
            let Some(found) = found else {
                break;
            };
            output.extend_from_slice(&text[last..found.range.start]);
            expand_substitution(&mut output, &text, &replacement, &found)?;
            if output.len() > TEXT_UNITS {
                return Err(Error::invalid("Replacement notes exceed 10000 characters"));
            }
            last = found.range.end;
            position = if found.range.is_empty() {
                found.range.end + 1
            } else {
                found.range.end
            };
        }
        output.extend_from_slice(&text[last..]);
        String::from_utf16(&output)
            .map_err(|_| Error::invalid("Replacement would produce an unpaired Unicode surrogate"))
    }
}
fn expand_substitution(
    output: &mut Vec<u16>,
    text: &[u16],
    replacement: &[u16],
    found: &regress::Match,
) -> Result<(), Error> {
    let mut i = 0;
    while i < replacement.len() {
        if output.len() > TEXT_UNITS {
            return Err(Error::invalid("Replacement notes exceed 10000 characters"));
        }
        if replacement[i] != u16::from(b'$') || i + 1 == replacement.len() {
            output.push(replacement[i]);
            i += 1;
            continue;
        }
        let next = replacement[i + 1];
        let range = match next {
            n if n == u16::from(b'$') => {
                output.push(n);
                i += 2;
                continue;
            }
            n if n == u16::from(b'&') => Some(found.range.clone()),
            n if n == u16::from(b'`') => Some(0..found.range.start),
            n if n == u16::from(b'\'') => Some(found.range.end..text.len()),
            n if n == u16::from(b'<') && found.named_groups().next().is_some() => {
                if let Some(end) = replacement[i + 2..]
                    .iter()
                    .position(|c| *c == u16::from(b'>'))
                {
                    let name = String::from_utf16_lossy(&replacement[i + 2..i + 2 + end]);
                    if let Some(range) = found.named_group(&name) {
                        output.extend_from_slice(&text[range]);
                    }
                    i += end + 3;
                    continue;
                } else {
                    None
                }
            }
            n if (u16::from(b'0')..=u16::from(b'9')).contains(&n) => {
                let mut group = (n - u16::from(b'0')) as usize;
                let mut digits = 1;
                if i + 2 < replacement.len()
                    && (u16::from(b'0')..=u16::from(b'9')).contains(&replacement[i + 2])
                {
                    let two = group * 10 + (replacement[i + 2] - u16::from(b'0')) as usize;
                    if two > 0 && two <= found.captures.len() {
                        group = two;
                        digits = 2;
                    }
                }
                if group > 0 && group <= found.captures.len() {
                    if let Some(range) = found.group(group) {
                        output.extend_from_slice(&text[range]);
                    }
                    i += digits + 1;
                    continue;
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(range) = range {
            output.extend_from_slice(&text[range]);
            i += 2;
        } else {
            output.push(replacement[i]);
            i += 1;
        }
    }
    if output.len() > TEXT_UNITS {
        return Err(Error::invalid("Replacement notes exceed 10000 characters"));
    }
    Ok(())
}
