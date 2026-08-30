//! A minimal valid ONNX model, hand-encoded as raw protobuf bytes — a
//! dummy fixture for exercising ONNX-backed code in tests, **not** a real
//! trained model. Declares a `"obs"` `[1, OBS_SIZE]` float input (unused —
//! neither output actually depends on it, which ONNX permits) and two
//! `Constant`-op nodes producing fixed `"policy"` `[1, ActionSpace::SIZE]`
//! (all-zero logits, so a masked softmax over them produces a uniform
//! distribution over whatever's legal) and `"value"` `[1, 1]` (`0.25`)
//! outputs. Only the handful of `onnx.proto3` fields this needs are
//! implemented — this is not a general protobuf encoder.
//!
//! Extracted out of `onnx_policy`'s own test module so both its unit tests
//! and this crate's `tests/agent_adapter_test.rs` integration test can
//! build a working `.onnx` file without duplicating this encoding — an
//! integration test only ever sees this crate's normal (non-`cfg(test)`)
//! build, so a `#[cfg(test)]`-gated fixture wouldn't be reachable from
//! there. Feature-gated behind `onnx` (same as everything else that needs
//! `ActionSpace`/`OBS_SIZE`-shaped ONNX I/O) rather than `cfg(test)`.

use std::io::Write;
use std::path::PathBuf;

use netrunner_core::rules::ActionSpace;

use crate::observation::OBS_SIZE;

fn varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

fn field_tag(field_number: u32, wire_type: u32) -> u64 {
    ((field_number as u64) << 3) | wire_type as u64
}

fn write_varint_field(out: &mut Vec<u8>, field_number: u32, value: u64) {
    varint(field_tag(field_number, 0), out);
    varint(value, out);
}

fn write_bytes_field(out: &mut Vec<u8>, field_number: u32, bytes: &[u8]) {
    varint(field_tag(field_number, 2), out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn write_string_field(out: &mut Vec<u8>, field_number: u32, value: &str) {
    write_bytes_field(out, field_number, value.as_bytes());
}

fn write_packed_varints(out: &mut Vec<u8>, field_number: u32, values: &[i64]) {
    let mut packed = Vec::new();
    for &value in values {
        varint(value as u64, &mut packed);
    }
    write_bytes_field(out, field_number, &packed);
}

fn write_packed_floats(out: &mut Vec<u8>, field_number: u32, values: &[f32]) {
    let mut packed = Vec::new();
    for &value in values {
        packed.extend_from_slice(&value.to_le_bytes());
    }
    write_bytes_field(out, field_number, &packed);
}

const ELEM_TYPE_FLOAT: i64 = 1;
const ATTRIBUTE_TYPE_TENSOR: u64 = 4;

fn tensor_shape(dims: &[i64]) -> Vec<u8> {
    let mut out = Vec::new();
    for &dim in dims {
        let mut dimension = Vec::new();
        write_varint_field(&mut dimension, 1, dim as u64); // Dimension.dim_value
        write_bytes_field(&mut out, 1, &dimension); // TensorShapeProto.dim
    }
    out
}

fn type_proto(elem_type: i64, dims: &[i64]) -> Vec<u8> {
    let mut tensor = Vec::new();
    write_varint_field(&mut tensor, 1, elem_type as u64); // Tensor.elem_type
    write_bytes_field(&mut tensor, 2, &tensor_shape(dims)); // Tensor.shape
    let mut type_proto = Vec::new();
    write_bytes_field(&mut type_proto, 1, &tensor); // TypeProto.tensor_type
    type_proto
}

fn value_info(name: &str, elem_type: i64, dims: &[i64]) -> Vec<u8> {
    let mut out = Vec::new();
    write_string_field(&mut out, 1, name); // ValueInfoProto.name
    write_bytes_field(&mut out, 2, &type_proto(elem_type, dims)); // ValueInfoProto.type
    out
}

fn constant_tensor(dims: &[i64], data: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    write_packed_varints(&mut out, 1, dims); // TensorProto.dims
    write_varint_field(&mut out, 2, ELEM_TYPE_FLOAT as u64); // TensorProto.data_type
    write_packed_floats(&mut out, 4, data); // TensorProto.float_data
    out
}

fn constant_node(output_name: &str, dims: &[i64], data: &[f32]) -> Vec<u8> {
    let mut attribute = Vec::new();
    write_string_field(&mut attribute, 1, "value"); // AttributeProto.name
    write_varint_field(&mut attribute, 20, ATTRIBUTE_TYPE_TENSOR); // AttributeProto.type
    write_bytes_field(&mut attribute, 5, &constant_tensor(dims, data)); // AttributeProto.t

    let mut node = Vec::new();
    write_string_field(&mut node, 2, output_name); // NodeProto.output
    write_string_field(&mut node, 4, "Constant"); // NodeProto.op_type
    write_bytes_field(&mut node, 5, &attribute); // NodeProto.attribute
    node
}

/// Builds the raw bytes of the dummy fixture model described above.
pub fn build_model_bytes() -> Vec<u8> {
    let policy_len = ActionSpace::SIZE as i64;
    let obs_len = OBS_SIZE as i64;

    let mut graph = Vec::new();
    write_bytes_field(&mut graph, 1, &constant_node("policy", &[1, policy_len], &vec![0.0; policy_len as usize])); // GraphProto.node
    write_bytes_field(&mut graph, 1, &constant_node("value", &[1, 1], &[0.25])); // GraphProto.node
    write_bytes_field(&mut graph, 11, &value_info("obs", ELEM_TYPE_FLOAT, &[1, obs_len])); // GraphProto.input
    write_bytes_field(&mut graph, 12, &value_info("policy", ELEM_TYPE_FLOAT, &[1, policy_len])); // GraphProto.output
    write_bytes_field(&mut graph, 12, &value_info("value", ELEM_TYPE_FLOAT, &[1, 1])); // GraphProto.output

    let mut opset = Vec::new();
    write_varint_field(&mut opset, 2, 13); // OperatorSetIdProto.version

    let mut model = Vec::new();
    write_varint_field(&mut model, 1, 8); // ModelProto.ir_version
    write_bytes_field(&mut model, 8, &opset); // ModelProto.opset_import
    write_bytes_field(&mut model, 7, &graph); // ModelProto.graph
    model
}

/// A throwaway `.onnx` file under the OS temp dir, cleaned up on `Drop` —
/// there's no `tempfile` dependency in this workspace, and one isn't worth
/// adding just for this.
pub struct TempOnnxFile {
    pub path: PathBuf,
}

impl TempOnnxFile {
    pub fn write(bytes: &[u8]) -> Self {
        // `std::process::id()` alone is constant for the whole test binary
        // run — the counter disambiguates multiple `TempOnnxFile`s written
        // concurrently (`cargo test` runs test functions in parallel
        // threads by default) from the same process.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("netrunner_bots_onnx_fixture_{}_{unique}.onnx", std::process::id()));
        let mut file = std::fs::File::create(&path).expect("can create a temp file for the ONNX fixture");
        file.write_all(bytes).expect("can write the ONNX fixture's bytes");
        TempOnnxFile { path }
    }
}

impl Drop for TempOnnxFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Convenience one-shot: builds the dummy model bytes and writes them to a
/// fresh temp file in one call.
pub fn write_fixture_model() -> TempOnnxFile {
    TempOnnxFile::write(&build_model_bytes())
}
