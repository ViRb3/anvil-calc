mod calc;

use std::panic;
use wasm_bindgen::prelude::*;
use crate::calc::{ConfigSchema, process};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u32(a: u32);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_many(a: &str, b: &str);
}

#[wasm_bindgen]
pub fn process_wasm(input: String) -> String {
    panic::set_hook(Box::new(console_error_panic_hook::hook));
    let start = instant::Instant::now();
    let config: ConfigSchema = serde_yaml::from_str(input.as_str()).expect("unable to parse input");
    let result = process(config);
    log(format!("Done in {}ms", start.elapsed().as_millis()).as_str());
    result
}