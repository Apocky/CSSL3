//! § Native CSSL CFG oracle: parser → HIR → MIR → x64 bytes → in-process call.
//! § No subprocess, shell, external service, object linker, or host decision fallback.
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use std::ffi::c_void;
use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_cgen_cpu_x64::{abi::X64Abi, isel::select::select_function, mb_walker::build_multi_block_func_bytes};
use cssl_mir::{lower_fn_body, lower_function_signature, validate_and_mark, LowerCtx, MirModule};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, allocation: u32, protect: u32) -> *mut c_void;
    fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
    fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
}

struct NativeCode { address: *mut c_void }
impl Drop for NativeCode {
    fn drop(&mut self) { unsafe { VirtualFree(self.address, 0, 0x8000); } }
}
impl NativeCode {
    fn compile(source_text: &str, name: &str) -> Self {
        let source = SourceFile::new(SourceId::first(), "<native-cfg-oracle>", source_text, Surface::RustHybrid);
        let tokens = cssl_lex::lex(&source);
        let (cst, parse_diagnostics) = cssl_parse::parse(&source, &tokens);
        assert!(!parse_diagnostics.has_errors(), "{parse_diagnostics:?}");
        let (hir, interner, lower_diagnostics) = cssl_hir::lower_module(&source, &cst);
        assert!(!lower_diagnostics.has_errors(), "{lower_diagnostics:?}");
        let mut module = MirModule::with_name("native_cfg_oracle");
        let context = LowerCtx::new(&interner);
        for item in &hir.items {
            if let cssl_hir::HirItem::Fn(function) = item {
                let mut lowered = lower_function_signature(&context, function);
                lower_fn_body(&interner, Some(&source), function, &mut lowered);
                module.push_func(lowered);
            }
        }
        validate_and_mark(&mut module).expect("structured CFG validation");
        let function = module.find_func(name).expect("source function");
        assert!(function.params.len() <= 4);
        assert!(function.params.iter().chain(&function.results).all(|ty| *ty == cssl_mir::MirType::Int(cssl_mir::IntWidth::I64)));
        assert_eq!(function.results.len(), 1);
        let selected = select_function(&module, function).expect("native selection");
        let code = build_multi_block_func_bytes(&selected, X64Abi::MicrosoftX64, true).expect("native bytes");
        assert!(code.relocs.is_empty());
        assert!(!code.bytes.is_empty() && code.bytes.len() <= 64 * 1024);
        // § RW copy → RX only; no RWX allocation, child process, or persisted executable.
        let address = unsafe { VirtualAlloc(std::ptr::null_mut(), code.bytes.len(), 0x3000, 0x04) };
        assert!(!address.is_null());
        let allocation = Self { address };
        unsafe { std::ptr::copy_nonoverlapping(code.bytes.as_ptr(), address.cast::<u8>(), code.bytes.len()); }
        let mut previous = 0;
        assert_ne!(unsafe { VirtualProtect(address, code.bytes.len(), 0x20, &mut previous) }, 0);
        assert_ne!(unsafe { FlushInstructionCache(GetCurrentProcess(), address, code.bytes.len()) }, 0);
        allocation
    }
    fn call(&self, a: i64, b: i64, c: i64, d: i64) -> i64 {
        // § x64 ABI admits four scalar register arguments; unused trailing registers
        // are ignored by the one-argument TTL source function.
        let function: unsafe extern "system" fn(i64, i64, i64, i64) -> i64 = unsafe { std::mem::transmute(self.address) };
        unsafe { function(a, b, c, d) }
    }
}
fn order(value: u64) -> i64 { (value ^ (1_u64 << 63)) as i64 }

#[test]
fn exact_capability_policy_native_truth_table() {
    let code = NativeCode::compile(include_str!("fixtures/metaharness_capability_decision.cssl"), "apoc_meta_capability_decide");
    let values = [0, 1, 2, (1_u64 << 63) - 1, 1_u64 << 63, (1_u64 << 63) + 1, u64::MAX - 1, u64::MAX];
    for flags in 0..64 {
        for issued in values { for expires in values { for now in values {
            let expected = if flags != 31 && flags != 15 { 1 }
                else if issued >= expires { 1 } else if now >= expires { 2 }
                else if flags == 31 { 0 } else { 3 };
            assert_eq!(code.call(flags, order(issued), order(expires), order(now)), expected,
                "flags={flags}, issued={issued}, expires={expires}, now={now}");
        } } }
    }
}

#[test]
fn exact_ttl_policy_native_unsigned_edges() {
    let code = NativeCode::compile(include_str!("fixtures/metaharness_capability_decision.cssl"), "apoc_meta_bootstrap_ttl");
    for ttl in [0, 1, 2, 86_399_999, 86_400_000, 86_400_001, 1_u64 << 63, u64::MAX] {
        assert_eq!(code.call(order(ttl), 0, 0, 0), i64::from((1..=86_400_000).contains(&ttl)), "ttl={ttl}");
    }
}

#[test]
fn nested_statement_branches_return_from_actual_exit() {
    let code = NativeCode::compile("pub fn nested(a: i64, b: i64, c: i64) -> i64 { if a == 3 { if b >= c { return 11; } if b == 1 { return 22; } return 33; } if a == 4 { return 44; } return 55; }", "nested");
    for a in 2..6 { for b in 0..3 { for c in 0..3 {
        let expected = if a == 3 { if b >= c { 11 } else if b == 1 { 22 } else { 33 } } else if a == 4 { 44 } else { 55 };
        assert_eq!(code.call(a,b,c,0), expected, "a={a}, b={b}, c={c}");
    } } }
}

#[test]
fn saved_boolean_survives_intervening_flag_clobber() {
    let code = NativeCode::compile("pub fn saved(a: i64, b: i64) -> i64 { let condition = a < b; let noise = a - b; if condition { return 7; } return noise; }", "saved");
    for (a,b) in [(1,2),(2,1),(3,3),(-5,2),(2,-5)] {
        assert_eq!(code.call(a,b,0,0), if a < b { 7 } else { a-b });
    }
}

#[test]
fn sequential_branches_reuse_dead_registers_without_losing_argument() {
    let mut source = "pub fn many(a: i64) -> i64 {".to_string();
    for value in 0..40 { source.push_str(&format!("if a == {value} {{ return {}; }}", value+1)); }
    source.push_str("return 91; }");
    let code = NativeCode::compile(&source, "many");
    for value in -1..42 { assert_eq!(code.call(value,0,0,0), if (0..40).contains(&value) { value+1 } else { 91 }); }
}

#[test]
fn expert_selection_preserves_low_byte_booleans_under_register_pressure() {
    let code = NativeCode::compile(include_str!("fixtures/expert_selection.cssl"), "apoc_v2_expert_top2");
    for a in -3..=2 { for b in -3..=2 { for c in -3..=2 { for d in -3..=2 {
        let values = [a, b, c, d];
        let mut ranked: Vec<(usize, i64)> = values.into_iter().enumerate().filter(|(_, score)| *score != -3).collect();
        ranked.sort_by(|(left_id, left), (right_id, right)| right.cmp(left).then(left_id.cmp(right_id)));
        let expected = if ranked.len() < 2 { -2 } else { (4 * ranked[0].0 + ranked[1].0) as i64 };
        assert_eq!(code.call(a, b, c, d), expected, "scores={values:?}");
    } } } }
    for slot in 0..4 { for invalid in [i64::MIN, -4, 3, i64::MAX] {
        let mut values = [-3; 4]; values[slot] = invalid;
        assert_eq!(code.call(values[0], values[1], values[2], values[3]), -1, "scores={values:?}");
    } }
}
