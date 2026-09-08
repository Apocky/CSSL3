import ctypes
import json
import sys


dll = ctypes.CDLL(sys.argv[1])
u8 = ctypes.c_uint8
u32 = ctypes.c_uint32

for name in ("bool_identity_v1", "bool_not_v1"):
    function = getattr(dll, name)
    function.argtypes = [u8]
    function.restype = u8

dll.bool_not_branch_v1.argtypes = [u8]
dll.bool_not_branch_v1.restype = u32
for name in ("bool_true_v1", "bool_false_v1"):
    function = getattr(dll, name)
    function.argtypes = []
    function.restype = u8
dll.bit_not_u8_v1.argtypes = [u8]
dll.bit_not_u8_v1.restype = u8

expected = {
    "identity_false": 0,
    "identity_true": 1,
    "not_false": 1,
    "not_true": 0,
    "branch_false": 11,
    "branch_true": 22,
    "literal_true": 1,
    "literal_false": 0,
    "bit_not_zero": 255,
    "bit_not_one": 254,
}
actual = {
    "identity_false": dll.bool_identity_v1(0),
    "identity_true": dll.bool_identity_v1(1),
    "not_false": dll.bool_not_v1(0),
    "not_true": dll.bool_not_v1(1),
    "branch_false": dll.bool_not_branch_v1(0),
    "branch_true": dll.bool_not_branch_v1(1),
    "literal_true": dll.bool_true_v1(),
    "literal_false": dll.bool_false_v1(),
    "bit_not_zero": dll.bit_not_u8_v1(0),
    "bit_not_one": dll.bit_not_u8_v1(1),
}
if actual != expected:
    raise AssertionError({"actual": actual, "expected": expected})

print(json.dumps({"status": "PASS", "assertions": len(expected), "values": actual}, sort_keys=True))
