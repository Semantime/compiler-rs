pub use compiler_bench::{
    AnalyzeRecord, AnalyzeRequest, AnalyzeResponse, RegressionCase, RegressionReport,
    RegressionViewerCase, RegressionViewerData, ScopeRegressionSummary, analyze_request,
    analyze_request_file, build_regression_viewer_data, build_regression_viewer_data_from_dir,
    default_demo_dir, default_regression_dir, load_analyze_requests_from_dir,
    load_regression_cases_from_dir, run_demo, run_demo_from_dir, run_regression_cases,
    run_regression_suite, run_regression_suite_from_dir,
};
pub use compiler_core::{
    CompilerPolicy, GroupInput, SensitivityProfile, analyze_groups, analyze_lines, project_output,
};
pub use compiler_schema::{
    CanonicalAnalysis, Event, EventKind, EvidenceItem, LlmOutput, LogicalSeries, NormalizedSeries,
    PeerContext, Regime, SCHEMA_VERSION, Scope, StateKind, Statistics, TimeSeriesPoint, TrendKind,
};

#[cfg(all(target_arch = "wasm32", feature = "wasm-bindgen-export"))]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use std::sync::{Mutex, OnceLock};

fn serialize_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

pub fn default_policy_json() -> Result<String, String> {
    serialize_json(&CompilerPolicy::default())
}

pub fn analyze_request_json(input: &str) -> Result<String, String> {
    analyze_request_json_with_policy(input, None)
}

pub fn analyze_request_json_with_policy(
    input: &str,
    default_policy_json: Option<&str>,
) -> Result<String, String> {
    let request: AnalyzeRequest = serde_json::from_str(input).map_err(|err| err.to_string())?;
    let policy = match default_policy_json {
        Some(raw) => serde_json::from_str(raw).map_err(|err| err.to_string())?,
        None => CompilerPolicy::default(),
    };
    let response = analyze_request(&request, &policy);
    serialize_json(&response)
}

#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct WasmBuffers {
    result: Vec<u8>,
    error: Vec<u8>,
}

#[cfg(target_arch = "wasm32")]
fn wasm_buffers() -> &'static Mutex<WasmBuffers> {
    static BUFFERS: OnceLock<Mutex<WasmBuffers>> = OnceLock::new();
    BUFFERS.get_or_init(|| Mutex::new(WasmBuffers::default()))
}

#[cfg(target_arch = "wasm32")]
fn set_wasm_success(result: String) -> i32 {
    let mut buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.result = result.into_bytes();
    buffers.error.clear();
    0
}

#[cfg(target_arch = "wasm32")]
fn set_wasm_error(error: String) -> i32 {
    let mut buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.result.clear();
    buffers.error = error.into_bytes();
    1
}

#[cfg(target_arch = "wasm32")]
fn read_wasm_input(ptr: *const u8, len: usize) -> Result<&'static [u8], String> {
    if ptr.is_null() {
        return Err("input pointer is null".into());
    }

    // The host allocates this buffer through `compiler_alloc`, so reading it as a
    // borrowed slice for the duration of the call is safe.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_analyze_json(ptr: *const u8, len: usize) -> i32 {
    let input = match read_wasm_input(ptr, len)
        .and_then(|bytes| std::str::from_utf8(bytes).map_err(|err| err.to_string()))
    {
        Ok(input) => input,
        Err(err) => return set_wasm_error(err),
    };

    match analyze_request_json(input) {
        Ok(result) => set_wasm_success(result),
        Err(err) => set_wasm_error(err),
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_default_policy_json() -> i32 {
    match default_policy_json() {
        Ok(result) => set_wasm_success(result),
        Err(err) => set_wasm_error(err),
    }
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_result_ptr() -> *const u8 {
    let buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.result.as_ptr()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_result_len() -> usize {
    let buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.result.len()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_error_ptr() -> *const u8 {
    let buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.error.as_ptr()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn compiler_error_len() -> usize {
    let buffers = wasm_buffers().lock().expect("wasm buffers lock poisoned");
    buffers.error.len()
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-bindgen-export"))]
#[wasm_bindgen(js_name = defaultPolicyJson)]
pub fn wasm_default_policy_json() -> Result<String, JsValue> {
    default_policy_json().map_err(|err| JsValue::from_str(&err))
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-bindgen-export"))]
#[wasm_bindgen(js_name = analyzeRequestJson)]
pub fn wasm_analyze_request_json(input: &str) -> Result<String, JsValue> {
    analyze_request_json(input).map_err(|err| JsValue::from_str(&err))
}

#[cfg(all(target_arch = "wasm32", feature = "wasm-bindgen-export"))]
#[wasm_bindgen(js_name = analyzeRequestJsonWithPolicy)]
pub fn wasm_analyze_request_json_with_policy(
    input: &str,
    default_policy_json: Option<String>,
) -> Result<String, JsValue> {
    analyze_request_json_with_policy(input, default_policy_json.as_deref())
        .map_err(|err| JsValue::from_str(&err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_request_json_round_trips_line_requests() {
        let input = r#"{
          "scope": "line",
          "series": [
            {
              "metric_id": "cpu",
              "entity_id": "host-1",
              "group_id": "cluster-a",
              "points": [
                {"ts_secs": 0, "value": 1.0},
                {"ts_secs": 60, "value": 1.1},
                {"ts_secs": 120, "value": 1.2},
                {"ts_secs": 180, "value": 4.5},
                {"ts_secs": 240, "value": 4.8}
              ]
            }
          ]
        }"#;

        let output = analyze_request_json(input).expect("json export should succeed");
        let response: serde_json::Value =
            serde_json::from_str(&output).expect("response json should deserialize");

        assert_eq!(response["outputs"].as_array().map(Vec::len), Some(1));
        assert_eq!(response["outputs"][0]["canonical"]["subject_id"], "host-1");
    }

    #[test]
    fn default_policy_json_is_valid_json() {
        let policy: CompilerPolicy =
            serde_json::from_str(&default_policy_json().expect("policy json should render"))
                .expect("policy json should deserialize");

        assert_eq!(policy, CompilerPolicy::default());
    }
}
