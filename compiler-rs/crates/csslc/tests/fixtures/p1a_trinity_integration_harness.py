import ctypes
import json
import sys


dll = ctypes.CDLL(sys.argv[1])
expected = {
    "abi9001_record_c_boundary_status_v1": 0,
    "abi9003_u128_c_boundary_status_v1": 0,
    "profile_probe_core_v1": 0,
    "profile_probe_u128_composite_v1": 0,
    "profile_probe_native_u128_scalar_v1": 0,
    "profile_probe_codec_shift_v1": 0,
    "profile_probe_unsigned_shift_semantics_v1": 0,
    "profile_probe_u64_cross_call_v1": 0,
    "profile_probe_record_v1": 0,
    "profile_variant_native_status_v1": 0,
    "profile_probe_variant_v1": 0,
    "profile_probe_known_refusals_v1": 0,
    "profile_probe_all_v1": 0,
}

actual = {}
for name, wanted in expected.items():
    function = getattr(dll, name)
    function.argtypes = []
    function.restype = ctypes.c_int32
    observed = function()
    actual[name] = observed
    if observed != wanted:
        raise AssertionError(f"{name}: actual={observed} expected={wanted}")

print(
    json.dumps(
        {"status": "PASS", "assertions": len(expected), "probes": actual},
        sort_keys=True,
    )
)
