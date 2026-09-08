import ctypes
import pathlib
import random
import sys


dll_path = pathlib.Path(sys.argv[1]).resolve()
dll = ctypes.CDLL(str(dll_path))
u8 = ctypes.c_uint8
u64 = ctypes.c_uint64
i32 = ctypes.c_int32

dll.abi9001_projected_u8_cast_v1.argtypes = [u8]
dll.abi9001_projected_u8_cast_v1.restype = u64
dll.abi9001_projected_u64_shift_v1.argtypes = [u64]
dll.abi9001_projected_u64_shift_v1.restype = u64
dll.abi9001_projected_u64_div_v1.argtypes = [u64]
dll.abi9001_projected_u64_div_v1.restype = u64
dll.abi9001_projected_u64_rem_v1.argtypes = [u64]
dll.abi9001_projected_u64_rem_v1.restype = u64
dll.abi9001_projected_u64_gt_v1.argtypes = [u64, u64]
dll.abi9001_projected_u64_gt_v1.restype = i32

checks = 0
for value in range(256):
    assert dll.abi9001_projected_u8_cast_v1(value) == value
    checks += 1

edges = [
    0,
    1,
    2,
    3,
    0x7FFFFFFFFFFFFFFF,
    0x8000000000000000,
    0x8000000000000001,
    0xFFFFFFFFFFFFFFFE,
    0xFFFFFFFFFFFFFFFF,
]
rng = random.Random(0xAB19001)
values = edges + [rng.getrandbits(64) for _ in range(4096)]
threshold = 0x7FFFFFFFFFFFFFFF
for value in values:
    assert dll.abi9001_projected_u64_shift_v1(value) == value >> 7
    assert dll.abi9001_projected_u64_div_v1(value) == value // 3
    assert dll.abi9001_projected_u64_rem_v1(value) == value % 3
    assert dll.abi9001_projected_u64_gt_v1(value, threshold) == int(value > threshold)
    checks += 4

assert dll.abi9001_projected_u64_gt_v1(0x8000000000000000, 1) == 1
assert dll.abi9001_projected_u64_gt_v1(0xFFFFFFFFFFFFFFFF, 0x8000000000000000) == 1
assert dll.abi9001_projected_u64_gt_v1(1, 0x8000000000000000) == 0
checks += 3

print(f"PASS {checks}/{checks}")
