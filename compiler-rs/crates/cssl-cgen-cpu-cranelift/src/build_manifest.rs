//! Build-artifact manifest emission + pre-link verification.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cssl_mir::{MirFunc, MirModule, MirOp, MirRegion, MirType, MirValue, ValueId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MANIFEST_SCHEMA: &str = "cssl.build-manifest.v1";
const MANIFEST_FILENAME: &str = "build-manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildManifest {
    pub schema: String,
    pub symbols: Vec<SymbolHash>,
}

impl BuildManifest {
    pub fn from_module(module: &MirModule) -> Self {
        let symbols = module
            .funcs
            .iter()
            .filter(|func| !func.is_generic)
            .map(SymbolHash::from_func)
            .collect();
        Self {
            schema: MANIFEST_SCHEMA.to_string(),
            symbols,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolHash {
    #[serde(rename = "symbol-name")]
    pub name: String,
    pub hash: String,
}

impl SymbolHash {
    pub fn new(name: impl Into<String>, hash: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hash: hash.into(),
        }
    }

    pub fn from_func(func: &MirFunc) -> Self {
        Self::new(func.name.clone(), hash_func(func))
    }
}

#[derive(Debug, Error)]
pub enum BuildManifestError {
    #[error("build-manifest.json missing: {path}")]
    Missing { path: String },

    #[error("manifest mismatch: symbol {symbol} expected hash {expected}, got {actual}")]
    Mismatch {
        symbol: String,
        expected: String,
        actual: String,
    },

    #[error("manifest missing symbol {symbol}")]
    MissingSymbol { symbol: String },

    #[error("manifest unexpected symbol {symbol}")]
    UnexpectedSymbol { symbol: String },

    #[error("manifest duplicate symbol {symbol}")]
    DuplicateSymbol { symbol: String },

    #[error("manifest schema mismatch: expected {expected}, got {actual}")]
    SchemaMismatch { expected: String, actual: String },

    #[error("manifest json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("manifest io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn emit_build_manifest_json(module: &MirModule) -> Result<String, BuildManifestError> {
    let manifest = BuildManifest::from_module(module);
    Ok(serde_json::to_string_pretty(&manifest)?)
}

pub fn write_build_manifest(
    dir: impl AsRef<Path>,
    module: &MirModule,
) -> Result<PathBuf, BuildManifestError> {
    let path = dir.as_ref().join(MANIFEST_FILENAME);
    std::fs::write(&path, emit_build_manifest_json(module)?)?;
    Ok(path)
}

pub fn read_build_manifest(path: impl AsRef<Path>) -> Result<BuildManifest, BuildManifestError> {
    let path = path.as_ref();
    if !path.is_file() {
        return Err(BuildManifestError::Missing {
            path: path.display().to_string(),
        });
    }
    let json = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

pub fn verify_manifest_file(
    path: impl AsRef<Path>,
    actual: &BuildManifest,
) -> Result<(), BuildManifestError> {
    let expected = read_build_manifest(path)?;
    verify_manifest(&expected, actual)
}

pub fn verify_manifest_json(
    expected_json: &str,
    actual: &BuildManifest,
) -> Result<(), BuildManifestError> {
    let expected = serde_json::from_str(expected_json)?;
    verify_manifest(&expected, actual)
}

pub fn verify_manifest(
    expected: &BuildManifest,
    actual: &BuildManifest,
) -> Result<(), BuildManifestError> {
    if expected.schema != MANIFEST_SCHEMA {
        return Err(BuildManifestError::SchemaMismatch {
            expected: MANIFEST_SCHEMA.to_string(),
            actual: expected.schema.clone(),
        });
    }
    if actual.schema != MANIFEST_SCHEMA {
        return Err(BuildManifestError::SchemaMismatch {
            expected: MANIFEST_SCHEMA.to_string(),
            actual: actual.schema.clone(),
        });
    }

    let expected_by_name = symbol_map(&expected.symbols)?;
    let actual_by_name = symbol_map(&actual.symbols)?;

    for (name, want_hash) in &expected_by_name {
        let Some(got_hash) = actual_by_name.get(name) else {
            return Err(BuildManifestError::MissingSymbol {
                symbol: name.clone(),
            });
        };
        if want_hash != got_hash {
            return Err(BuildManifestError::Mismatch {
                symbol: name.clone(),
                expected: want_hash.clone(),
                actual: got_hash.clone(),
            });
        }
    }

    for name in actual_by_name.keys() {
        if !expected_by_name.contains_key(name) {
            return Err(BuildManifestError::UnexpectedSymbol {
                symbol: name.clone(),
            });
        }
    }

    Ok(())
}

fn symbol_map(symbols: &[SymbolHash]) -> Result<BTreeMap<String, String>, BuildManifestError> {
    let mut out = BTreeMap::new();
    for symbol in symbols {
        if out
            .insert(symbol.name.clone(), symbol.hash.clone())
            .is_some()
        {
            return Err(BuildManifestError::DuplicateSymbol {
                symbol: symbol.name.clone(),
            });
        }
    }
    Ok(out)
}

fn hash_func(func: &MirFunc) -> String {
    let mut hasher = blake3::Hasher::new();
    update_str(&mut hasher, MANIFEST_SCHEMA);
    update_str(&mut hasher, &func.name);
    update_types(&mut hasher, "params", &func.params);
    update_types(&mut hasher, "results", &func.results);
    update_opt(&mut hasher, "effect", func.effect_row.as_deref());
    update_opt(&mut hasher, "cap", func.cap.as_deref());
    update_opt(&mut hasher, "ifc", func.ifc_label.as_deref());
    update_attrs(&mut hasher, &func.attributes);
    update_region(&mut hasher, &func.body);
    hasher.finalize().to_hex().to_string()
}

fn update_region(hasher: &mut blake3::Hasher, region: &MirRegion) {
    update_str(hasher, "region");
    update_str(hasher, &region.blocks.len().to_string());
    for block in &region.blocks {
        update_str(hasher, "block");
        update_str(hasher, &block.label);
        update_values(hasher, "args", &block.args);
        update_str(hasher, &block.ops.len().to_string());
        for op in &block.ops {
            update_op(hasher, op);
        }
    }
}

fn update_op(hasher: &mut blake3::Hasher, op: &MirOp) {
    update_str(hasher, "op");
    update_str(hasher, &op.name);
    update_str(hasher, &format!("{:?}", op.op));
    update_value_ids(hasher, "operands", &op.operands);
    update_values(hasher, "results", &op.results);
    update_attrs(hasher, &op.attributes);
    update_str(hasher, &op.regions.len().to_string());
    for region in &op.regions {
        update_region(hasher, region);
    }
}

fn update_attrs(hasher: &mut blake3::Hasher, attrs: &[(String, String)]) {
    let mut sorted = attrs.to_vec();
    sorted.sort();
    update_str(hasher, "attrs");
    update_str(hasher, &sorted.len().to_string());
    for (key, value) in sorted {
        update_str(hasher, &key);
        update_str(hasher, &value);
    }
}

fn update_types(hasher: &mut blake3::Hasher, label: &str, types: &[MirType]) {
    update_str(hasher, label);
    update_str(hasher, &types.len().to_string());
    for ty in types {
        update_str(hasher, &ty.to_string());
    }
}

fn update_values(hasher: &mut blake3::Hasher, label: &str, values: &[MirValue]) {
    update_str(hasher, label);
    update_str(hasher, &values.len().to_string());
    for value in values {
        update_value_id(hasher, value.id);
        update_str(hasher, &value.ty.to_string());
    }
}

fn update_value_ids(hasher: &mut blake3::Hasher, label: &str, values: &[ValueId]) {
    update_str(hasher, label);
    update_str(hasher, &values.len().to_string());
    for value in values {
        update_value_id(hasher, *value);
    }
}

fn update_value_id(hasher: &mut blake3::Hasher, value: ValueId) {
    update_str(hasher, &value.0.to_string());
}

fn update_opt(hasher: &mut blake3::Hasher, label: &str, value: Option<&str>) {
    update_str(hasher, label);
    update_str(hasher, value.unwrap_or("<none>"));
}

fn update_str(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(value.as_bytes());
    hasher.update(&[0]);
}

#[cfg(test)]
mod tests {
    use super::{
        emit_build_manifest_json, read_build_manifest, verify_manifest, write_build_manifest,
        BuildManifest, BuildManifestError, SymbolHash,
    };
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};

    #[test]
    fn a04_1_emits_build_manifest_json_with_symbol_hash_tuple() {
        let module = module_with_answer(42);
        let json = emit_build_manifest_json(&module).expect("manifest json");
        assert!(json.contains("\"schema\": \"cssl.build-manifest.v1\""));
        assert!(json.contains("\"symbol-name\": \"answer\""));
        assert!(json.contains("\"hash\":"));
    }

    #[test]
    fn a04_2_manifest_mismatch_diagnostic_names_symbol_and_hashes() {
        let expected = BuildManifest {
            schema: "cssl.build-manifest.v1".to_string(),
            symbols: vec![SymbolHash::new("answer", "H")],
        };
        let actual = BuildManifest {
            schema: "cssl.build-manifest.v1".to_string(),
            symbols: vec![SymbolHash::new("answer", "H'")],
        };
        let err = verify_manifest(&expected, &actual).unwrap_err();
        assert!(matches!(err, BuildManifestError::Mismatch { .. }));
        assert_eq!(
            err.to_string(),
            "manifest mismatch: symbol answer expected hash H, got H'"
        );
    }

    #[test]
    fn a04_3_missing_manifest_fails_ci_gate_preflight() {
        let missing = temp_manifest_path("missing");
        let err = read_build_manifest(&missing).unwrap_err();
        assert!(matches!(err, BuildManifestError::Missing { .. }));
        assert!(err.to_string().contains("build-manifest.json missing"));
    }

    #[test]
    fn write_then_read_manifest_roundtrips() {
        let dir = temp_manifest_dir();
        std::fs::create_dir_all(&dir).expect("temp manifest dir");
        let path = write_build_manifest(&dir, &module_with_answer(7)).expect("write manifest");
        let read = read_build_manifest(&path).expect("read manifest");
        assert_eq!(read.symbols.len(), 1);
        assert_eq!(read.symbols[0].name, "answer");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }

    #[test]
    fn manifest_hash_changes_when_function_body_changes() {
        let a = BuildManifest::from_module(&module_with_answer(7));
        let b = BuildManifest::from_module(&module_with_answer(8));
        assert_ne!(a.symbols[0].hash, b.symbols[0].hash);
    }

    fn module_with_answer(value: i32) -> MirModule {
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut func = MirFunc::new("answer", vec![], vec![i32_ty.clone()]);
        func.next_value_id = 1;
        func.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), i32_ty)
                .with_attribute("value", value.to_string()),
        );
        func.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        let mut module = MirModule::new();
        module.push_func(func);
        module
    }

    fn temp_manifest_dir() -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "cssl_manifest_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        path
    }

    fn temp_manifest_path(label: &str) -> std::path::PathBuf {
        temp_manifest_dir().join(label).join("build-manifest.json")
    }
}
