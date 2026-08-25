//! `ort`-backed `PolicyEvaluator`: runs a trained ONNX policy/value network
//! instead of `UniformPolicyEvaluator`'s no-network baseline. Feature-gated
//! (`onnx`) since it pulls in `ort`/`ndarray` and — via `ort`'s
//! `download-binaries` feature — a prebuilt ONNX Runtime shared library,
//! none of which `PuctAgent`'s search machinery or the baseline evaluator
//! need.

use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use netrunner_core::cards::CardRegistry;
use netrunner_core::rules::{get_action_mask, ActionSpace, GameState, Side};

use crate::observation::{encode_observation, OBS_SIZE};
use crate::policy::PolicyEvaluator;

#[derive(Debug, thiserror::Error)]
pub enum OnnxPolicyError {
    #[error(transparent)]
    Ort(#[from] ort::Error),
    #[error("ONNX model \"policy\" output has {actual} element(s), expected ActionSpace::SIZE ({expected})")]
    UnexpectedPolicyShape { actual: usize, expected: usize },
    #[error("ONNX model \"value\" output has {actual} element(s), expected exactly 1")]
    UnexpectedValueShape { actual: usize },
}

/// A `PolicyEvaluator` backed by a trained ONNX model: input `"obs"` shape
/// `[1, OBS_SIZE]` (from `crate::observation::encode_observation`), outputs
/// `"policy"` shape `[1, ActionSpace::SIZE]` (raw logits — see
/// `masked_softmax`'s doc comment for why these aren't assumed
/// pre-softmaxed) and `"value"` shape `[1, 1]`.
pub struct OnnxPolicyEvaluator {
    // `ort::Session::run` needs `&mut self`, but `PolicyEvaluator::
    // evaluate` only gets `&self` (matching `UniformPolicyEvaluator` and
    // every other evaluator — not being changed just for this one
    // implementation). `Session` is already internally `Send + Sync` (a
    // thread-safe C API handle), so this `Mutex` only bridges that
    // `&self`/`&mut self` mismatch — it isn't adding any cross-thread
    // safety concern beyond what calling into ONNX Runtime from multiple
    // threads would already involve.
    session: Mutex<Session>,
    side: Side,
}

impl OnnxPolicyEvaluator {
    /// Loads the ONNX model at `model_path` and builds a session for it.
    /// `side` is fixed at construction (matching `UniformPolicyEvaluator::
    /// new(side)`) since `PolicyEvaluator::evaluate` itself receives no
    /// `side` parameter — this evaluator always encodes `state` from that
    /// one perspective.
    pub fn new(model_path: &str, side: Side) -> Result<Self, OnnxPolicyError> {
        let session = Session::builder()?.commit_from_file(model_path)?;
        Ok(Self { session: Mutex::new(session), side })
    }
}

impl PolicyEvaluator for OnnxPolicyEvaluator {
    fn evaluate(&self, state: &GameState, registry: &CardRegistry) -> (Vec<f32>, f32) {
        let obs = encode_observation(state, registry, self.side);
        let mask = get_action_mask(state, registry);

        let input = ndarray::Array2::from_shape_vec((1, OBS_SIZE), obs).expect("encode_observation always returns OBS_SIZE features");
        let input_tensor = Tensor::from_array(input).expect("a [1, OBS_SIZE] f32 array is always a valid tensor");

        let mut session = self.session.lock().expect("ONNX session mutex should never be poisoned");
        let outputs =
            session.run(ort::inputs!["obs" => input_tensor]).expect("ONNX inference failed on an already-loaded model");

        let (_, policy_logits) = outputs["policy"].try_extract_tensor::<f32>().expect("model's \"policy\" output must be an f32 tensor");
        assert_eq!(
            policy_logits.len(),
            ActionSpace::SIZE,
            "model's \"policy\" output must have exactly ActionSpace::SIZE elements"
        );

        let (_, value_slice) = outputs["value"].try_extract_tensor::<f32>().expect("model's \"value\" output must be an f32 tensor");
        assert_eq!(value_slice.len(), 1, "model's \"value\" output must have exactly 1 element");

        let priors = masked_softmax(policy_logits, &mask);
        (priors, value_slice[0])
    }
}

/// Softmax computed only over `mask`'s legal indices — illegal indices get
/// exactly `0.0` and never absorb any of the exp'd mass, and the legal
/// subset sums to `1.0`. This is deliberately *not* "softmax over
/// everything, then zero the illegal entries": that would leave the legal
/// subset summing to less than `1.0` whenever the model places nonzero
/// mass on actions it has no way to know are illegal — the model has no
/// notion of game rules, only `get_action_mask` does. Numerically stable
/// (shifts by the max legal logit before exponentiating). An all-illegal
/// mask (or empty input) returns an all-`0.0` vector rather than dividing
/// by zero.
fn masked_softmax(logits: &[f32], mask: &[bool]) -> Vec<f32> {
    debug_assert_eq!(logits.len(), mask.len());

    let max_legal =
        logits.iter().zip(mask).filter_map(|(&logit, &legal)| legal.then_some(logit)).fold(f32::NEG_INFINITY, f32::max);

    if !max_legal.is_finite() {
        return vec![0.0; logits.len()];
    }

    let exp: Vec<f32> =
        logits.iter().zip(mask).map(|(&logit, &legal)| if legal { (logit - max_legal).exp() } else { 0.0 }).collect();
    let sum: f32 = exp.iter().sum();
    if sum <= 0.0 {
        return vec![0.0; logits.len()];
    }
    exp.into_iter().map(|value| value / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- `masked_softmax`: pure-function tests, no `ort`/ONNX Runtime
    // involved at all — the primary, always-reliable coverage for this
    // module's masking logic. ---

    #[test]
    fn sums_to_one_over_the_legal_subset_and_is_zero_elsewhere() {
        let logits = vec![1.0, 5.0, -3.0, 0.5];
        let mask = vec![true, false, true, false];

        let priors = masked_softmax(&logits, &mask);
        assert_eq!(priors.len(), 4);
        assert_eq!(priors[1], 0.0);
        assert_eq!(priors[3], 0.0);
        assert!(priors[0] > 0.0 && priors[2] > 0.0);
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "sum was {sum}");
    }

    #[test]
    fn uniform_logits_yield_a_uniform_distribution_over_legal_actions() {
        let logits = vec![0.0; 6];
        let mask = vec![true, true, false, true, false, false];

        let priors = masked_softmax(&logits, &mask);
        let legal_count = mask.iter().filter(|&&legal| legal).count() as f32;
        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!((priors[index] - 1.0 / legal_count).abs() < 1e-6);
            } else {
                assert_eq!(priors[index], 0.0);
            }
        }
    }

    #[test]
    fn all_illegal_mask_returns_all_zero_without_panicking() {
        let logits = vec![1.0, 2.0, 3.0];
        let mask = vec![false, false, false];

        let priors = masked_softmax(&logits, &mask);
        assert_eq!(priors, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn is_stable_under_extreme_logits() {
        let logits = vec![1e30, -1e30, 500.0];
        let mask = vec![true, true, true];

        let priors = masked_softmax(&logits, &mask);
        assert!(priors.iter().all(|value| value.is_finite()));
        let sum: f32 = priors.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "sum was {sum}");
    }

    // --- Constructor error path: real `ort`/ONNX Runtime involved, but no
    // model file needed. ---

    #[test]
    fn new_with_a_nonexistent_model_path_returns_err_not_a_panic() {
        let result = OnnxPolicyEvaluator::new("/nonexistent/path/to/model.onnx", Side::Corp);
        assert!(result.is_err());
    }

    // --- Full construct-and-`evaluate` round trip against a hand-built
    // fixture model (no Python/torch/onnx tooling is available in this
    // sandbox — see `fixture` module below for how it's built). ---

    mod fixture {
        //! A minimal valid ONNX model, hand-encoded as raw protobuf bytes:
        //! declares a `"obs"` `[1, OBS_SIZE]` float input (unused — neither
        //! output actually depends on it, which ONNX permits) and two
        //! `Constant`-op nodes producing fixed `"policy"` `[1,
        //! ActionSpace::SIZE]` (all-zero logits, so `masked_softmax` should
        //! produce a uniform distribution over whatever's legal) and
        //! `"value"` `[1, 1]` (`0.25`) outputs. Only the handful of
        //! `onnx.proto3` fields this needs are implemented — this is not a
        //! general protobuf encoder.

        use super::*;

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

        pub(super) fn build_model_bytes() -> Vec<u8> {
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
    }

    fn write_fixture_model() -> tempfile_like::TempOnnxFile {
        tempfile_like::TempOnnxFile::write(&fixture::build_model_bytes())
    }

    /// A tiny self-contained "write bytes to a unique temp path, delete on
    /// drop" helper — this crate has no `tempfile` dependency, and adding
    /// one just for a single test file felt like more than this needs.
    mod tempfile_like {
        use std::io::Write;

        pub(super) struct TempOnnxFile {
            pub(super) path: std::path::PathBuf,
        }

        impl TempOnnxFile {
            pub(super) fn write(bytes: &[u8]) -> Self {
                let path = std::env::temp_dir().join(format!("netrunner_bots_onnx_fixture_{}.onnx", std::process::id()));
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
    }

    #[test]
    fn constructs_and_evaluates_against_a_fixture_model() {
        let model_file = write_fixture_model();
        let evaluator = OnnxPolicyEvaluator::new(model_file.path.to_str().unwrap(), Side::Runner)
            .expect("hand-built fixture model should load successfully");

        let registry = CardRegistry::new();
        let state = GameState::new(0);
        let mask = get_action_mask(&state, &registry);

        let (priors, value) = evaluator.evaluate(&state, &registry);

        assert_eq!(priors.len(), ActionSpace::SIZE);
        assert!((value - 0.25).abs() < 1e-5, "expected the fixture's constant 0.25 value, got {value}");

        let legal_count = mask.iter().filter(|&&legal| legal).count() as f32;
        for (index, &legal) in mask.iter().enumerate() {
            if legal {
                assert!((priors[index] - 1.0 / legal_count).abs() < 1e-4, "index {index}: expected uniform prior, got {}", priors[index]);
            } else {
                assert_eq!(priors[index], 0.0, "index {index} is illegal but got a nonzero prior");
            }
        }
    }
}
