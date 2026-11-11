//! Trusted planning code only. JSON input never becomes executable source.
use crate::error::Error;
use rquickjs::{Context, Ctx, Module, Runtime, WriteOptions};
use serde_json::Value;
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const SOURCE: &str = include_str!("vendor/simulation.js");
static COMPILED: OnceLock<Vec<u8>> = OnceLock::new();
fn failure(ctx: &Ctx<'_>, error: rquickjs::Error) -> Error {
    let detail = if error.is_exception() {
        ctx.catch()
            .as_exception()
            .and_then(|error| error.message())
            .unwrap_or_else(|| "Runtime limit reached".into())
    } else {
        error.to_string()
    };
    Error::invalid(format!(
        "Monte Carlo calculation failed or exceeded its limits: {detail}"
    ))
}
fn runtime(cancel: Arc<AtomicBool>) -> Result<Runtime, Error> {
    let rt = Runtime::new().map_err(Error::internal)?;
    rt.set_memory_limit(64 * 1024 * 1024);
    rt.set_max_stack_size(1024 * 1024);
    let deadline = Instant::now() + Duration::from_secs(20);
    rt.set_interrupt_handler(Some(Box::new(move || {
        cancel.load(Ordering::Relaxed) || Instant::now() > deadline
    })));
    Ok(rt)
}
fn bytecode() -> Result<&'static [u8], Error> {
    if let Some(compiled) = COMPILED.get() {
        return Ok(compiled);
    }
    let rt = runtime(Arc::new(AtomicBool::new(false)))?;
    let context = Context::full(&rt).map_err(Error::internal)?;
    let compiled = context.with(|ctx| {
        let module = Module::declare(
            ctx.clone(),
            "monte-carlo",
            format!("{SOURCE};globalThis.ActualMonteCarlo=ActualMonteCarlo;"),
        )
        .map_err(|error| failure(&ctx, error))?;
        module
            .write(WriteOptions::default())
            .map_err(|error| failure(&ctx, error))
    })?;
    let _ = COMPILED.set(compiled);
    Ok(COMPILED.get().unwrap())
}
pub(super) fn calculate(input: &Value, cancel: Arc<AtomicBool>) -> Result<Value, Error> {
    let compiled = bytecode()?;
    let rt = runtime(cancel)?;
    let context = Context::full(&rt).map_err(Error::internal)?;
    context.with(|ctx| {
        // Bytecode is compiled above from the immutable vendored asset, never input.
        let module = unsafe { Module::load(ctx.clone(), compiled) }.map_err(|error| failure(&ctx,error))?;
        module.eval().map_err(|error| failure(&ctx,error))?.1.finish::<()>().map_err(|error| failure(&ctx,error))?;
        ctx.globals().set("__input", input.to_string()).map_err(Error::internal)?;
        let output: String = ctx.eval(r#"{
          const input=JSON.parse(__input);
          const config=ActualMonteCarlo.monteCarloConfigFromMeta(input.config);
          const result=ActualMonteCarlo.runMonteCarloSimulation({...config,
            horizonYears:ActualMonteCarlo.getMonteCarloHorizonYears(config), seed:input.seed,
            captureRunDetail:input.captureRunDetail ?? undefined,
            deflateToTodaysMoney:input.deflateToTodaysMoney});
          JSON.stringify({config,result},(_key,value)=>ArrayBuffer.isView(value)?Array.from(value):value);
        }"#).map_err(|error| failure(&ctx,error))?;
        if output.len() > 8 * 1024 * 1024 { return Err(Error::invalid("Monte Carlo output exceeds8MiB")); }
        serde_json::from_str(&output).map_err(Error::internal)
    })
}
