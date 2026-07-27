mod calc;

pub use calc::{ConfigSchema, process};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
unsafe extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn process_wasm(input: &str) -> Result<String, JsError> {
    console_error_panic_hook::set_once();
    let start = performance_now();
    let config: ConfigSchema = yaml_serde::from_str(input)
        .map_err(|error| JsError::new(&format!("unable to parse input: {error}")))?;
    let result = process(config);
    log(&format!("Done in {:.0}ms", performance_now() - start));
    Ok(result)
}
