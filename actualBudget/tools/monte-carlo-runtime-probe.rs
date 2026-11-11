// Isolated feasibility probe. No database, host callbacks, filesystem API or network in JS.
use rquickjs::{Context, Runtime};
use std::time::{Duration, Instant};
fn main() {
    let script = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let started = Instant::now();
    let rt = Runtime::new().unwrap();
    rt.set_memory_limit(64 * 1024 * 1024);
    rt.set_max_stack_size(1024 * 1024);
    rt.set_interrupt_handler(Some(Box::new(move || {
        started.elapsed() > Duration::from_secs(20)
    })));
    let context = Context::full(&rt).unwrap();
    let result = context.with(|ctx| match ctx.eval::<String, _>(script) {
        Ok(result) => result,
        Err(error) => panic!("{error}: {:?}", ctx.catch()),
    });
    // Encode the nested JSON string without adding a serde dependency to the probe.
    let quoted = context.with(|ctx| {
        ctx.globals().set("result", result).unwrap();
        ctx.eval::<String, _>("JSON.stringify(result)").unwrap()
    });
    println!(
        "{{\"elapsed_ms\":{},\"result\":{}}}",
        started.elapsed().as_millis(),
        quoted
    );
}
