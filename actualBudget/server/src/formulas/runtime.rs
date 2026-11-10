//! Trusted, host-isolated spreadsheet engine. Formula text is parsed by
//! HyperFormula; it is never executable JavaScript. Each command has fresh state.
use crate::error::Error;
use icu_collator::{
    Collator, CollatorBorrowed,
    options::{AlternateHandling, CaseLevel, CollatorOptions, Strength},
    preferences::CollationCaseFirst,
};
use icu_locale::Locale;
use rquickjs::{Context, Ctx, Exception, Function, Module, Runtime, WriteOptions};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::OnceLock,
    time::{Duration, Instant},
};
const HEAP: usize = 64 * 1024 * 1024;
const SOURCE: &str = include_str!("vendor/hyperformula-3.5.0.min.js");
const FUNCTIONS: &str = include_str!("vendor/custom-functions.js");
const LANGUAGE: &str = include_str!("vendor/enUS-3.5.0.json");
static COMPILED: OnceLock<Vec<u8>> = OnceLock::new();
fn failure(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    if error.is_exception() {
        ctx.catch()
            .as_exception()
            .and_then(|e| e.message())
            .unwrap_or_else(|| "Formula runtime limit or engine error".into())
    } else {
        error.to_string()
    }
}
fn runtime() -> Result<Runtime, String> {
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(HEAP);
    rt.set_max_stack_size(512 * 1024);
    let deadline = Instant::now() + Duration::from_secs(2);
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    Ok(rt)
}
fn bytecode() -> Result<&'static [u8], Error> {
    if let Some(bytes) = COMPILED.get() {
        return Ok(bytes);
    }
    let compile = || -> Result<Vec<u8>, String> {
        let rt = runtime()?;
        let context = Context::full(&rt).map_err(|e| e.to_string())?;
        context.with(|ctx| {
            let module=Module::declare(ctx.clone(),"trusted-hyperformula",format!("var self=globalThis;{SOURCE};HyperFormula.HyperFormula.registerLanguage('enUS',{LANGUAGE});{FUNCTIONS};globalThis.ActualFormulaFunctions=ActualFormulaFunctions;HyperFormula.HyperFormula.registerFunctionPlugin(ActualFormulaFunctions.CustomFunctionsPlugin,ActualFormulaFunctions.customFunctionsTranslations);"))
                .map_err(|e|failure(&ctx,e))?;
            module.write(WriteOptions::default()).map_err(|e|failure(&ctx,e))
        })
    };
    // Resource contention must not permanently poison initialization. Cache only
    // successful trusted bytecode; a failed request remains safely retryable.
    let bytes = compile().map_err(|_| {
        Error::invalid("Trusted formula engine could not initialize within its resource limits")
    })?;
    let _ = COMPILED.set(bytes);
    Ok(COMPILED.get().unwrap())
}
const BOOTSTRAP: &str = r#"
globalThis.Intl=Object.freeze({Collator:class {
 constructor(locale, options={}) {
  const id=__collator(String(locale||'en-US'),String(options.sensitivity||'variant'),String(options.caseFirst||'false'),!!options.ignorePunctuation);
  this.compare=(a,b)=>__compare(id,String(a),String(b));
 }
}});
"#;
const EVALUATE: &str = include_str!("evaluate.js");
const CLOCK: &str = r#"
{
 const NativeDate = Date, now = JSON.parse(__input).local_clock_ms;
 globalThis.Date = class extends NativeDate {
  constructor(...args) { super(...(args.length ? args : [now])); }
  static now() { return now; }
  getFullYear() { return this.getUTCFullYear(); }
  getMonth() { return this.getUTCMonth(); }
  getDate() { return this.getUTCDate(); }
  getDay() { return this.getUTCDay(); }
  getHours() { return this.getUTCHours(); }
  getMinutes() { return this.getUTCMinutes(); }
  getSeconds() { return this.getUTCSeconds(); }
  getMilliseconds() { return this.getUTCMilliseconds(); }
  getTimezoneOffset() { return 0; }
 };
}
"#;
pub fn evaluate(
    input: &Value,
    mut prefetch: impl FnMut(&Value) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let compiled = bytecode()?;
    let rt = runtime().map_err(Error::invalid)?;
    let context = Context::full(&rt).map_err(|_| Error::invalid("Formula context unavailable"))?;
    context.with(|ctx| -> Result<Value, Error> {
        let collators = Rc::new(RefCell::new(Vec::<CollatorBorrowed<'static>>::new()));
        let create = collators.clone();
        ctx.globals()
            .set(
                "__collator",
                Function::new(
                    ctx.clone(),
                    move |ctx: Ctx<'_>,
                          locale: String,
                          sensitivity: String,
                          first: String,
                          punctuation: bool|
                          -> rquickjs::Result<usize> {
                        if locale.len() > 100 || create.borrow().len() >= 16 {
                            return Err(Exception::throw_range(&ctx, "Collation limit reached"));
                        }
                        let locale: Locale = locale
                            .parse()
                            .map_err(|_| Exception::throw_type(&ctx, "Invalid formula locale"))?;
                        let mut preferences = icu_collator::CollatorPreferences::from(locale);
                        preferences.case_first = Some(match first.as_str() {
                            "upper" => CollationCaseFirst::Upper,
                            "lower" => CollationCaseFirst::Lower,
                            _ => CollationCaseFirst::False,
                        });
                        let mut options = CollatorOptions::default();
                        options.strength = Some(match sensitivity.as_str() {
                            "base" | "case" => Strength::Primary,
                            "accent" => Strength::Secondary,
                            _ => Strength::Tertiary,
                        });
                        if sensitivity == "case" {
                            options.case_level = Some(CaseLevel::On);
                        }
                        options.alternate_handling = Some(if punctuation {
                            AlternateHandling::Shifted
                        } else {
                            AlternateHandling::NonIgnorable
                        });
                        let collator = Collator::try_new(preferences, options).map_err(|_| {
                            Exception::throw_type(&ctx, "Unsupported formula locale")
                        })?;
                        let mut all = create.borrow_mut();
                        all.push(collator);
                        Ok(all.len() - 1)
                    },
                )
                .map_err(Error::internal)?,
            )
            .map_err(Error::internal)?;
        let work = Rc::new(RefCell::new(0usize));
        ctx.globals()
            .set(
                "__compare",
                Function::new(
                    ctx.clone(),
                    move |ctx: Ctx<'_>,
                          id: usize,
                          left: String,
                          right: String|
                          -> rquickjs::Result<i32> {
                        let mut used = work.borrow_mut();
                        *used = used.saturating_add(left.len() + right.len() + 1);
                        if *used > 4 * 1024 * 1024 || left.len() > 20000 || right.len() > 20000 {
                            return Err(Exception::throw_range(
                                &ctx,
                                "Formula comparison work limit reached",
                            ));
                        }
                        let all = collators.borrow();
                        let col = all
                            .get(id)
                            .ok_or_else(|| Exception::throw_type(&ctx, "Unknown collator"))?;
                        Ok(col.compare(&left, &right) as i32)
                    },
                )
                .map_err(Error::internal)?,
            )
            .map_err(Error::internal)?;
        ctx.eval::<(), _>(BOOTSTRAP)
            .map_err(|e| Error::invalid(failure(&ctx, e)))?;
        // Only bytecode compiled in this process from the embedded, pinned bundle
        // enters QuickJS's unchecked bytecode reader. Never accept caller bytes.
        unsafe { Module::load(ctx.clone(), compiled) }
            .and_then(|m| m.eval())
            .and_then(|(_, p)| p.finish::<()>())
            .map_err(|e| Error::invalid(failure(&ctx, e)))?;
        ctx.globals()
            .set("__input", input.to_string())
            .map_err(Error::internal)?;
        ctx.eval::<(), _>(CLOCK)
            .map_err(|e| Error::invalid(failure(&ctx, e)))?;
        let mut updates = json!({});
        for phase in 0..3 {
            ctx.globals()
                .set("__phase", phase)
                .map_err(Error::internal)?;
            ctx.globals()
                .set("__updates", updates.to_string())
                .map_err(Error::internal)?;
            let output: String = ctx
                .eval(EVALUATE)
                .map_err(|e| Error::invalid(failure(&ctx, e)))?;
            if output.len() > 65536 {
                return Err(Error::invalid("Formula result exceeds64KiB"));
            }
            let result: Value = serde_json::from_str(&output).map_err(Error::internal)?;
            if phase == 2 || !result["output"]["error"].is_null() {
                return Ok(result["output"].clone());
            }
            updates = prefetch(&if phase == 0 {
                {
                    let mut requests = result["requests"].clone();
                    requests["budgetQueryRequests"] = json!([]);
                    requests
                }
            } else {
                json!({"budgetQueryRequests":result["requests"]["budgetQueryRequests"]})
            })?;
        }
        unreachable!()
    })
}
