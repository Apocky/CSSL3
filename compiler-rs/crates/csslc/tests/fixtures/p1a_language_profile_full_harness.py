import ctypes
import hashlib
import json
import random
import struct
import sys


dll = ctypes.CDLL(sys.argv[1])
U8 = ctypes.c_uint8
U32 = ctypes.c_uint32
U64 = ctypes.c_uint64
I32 = ctypes.c_int32
I64 = ctypes.c_int64
P8 = ctypes.POINTER(U8)

selectors = (
    0x00030002,
    0x00020000,
    0x00030000,
    0x00010000,
    1,
    1,
    0,
    0,
    1,
    0x00010000,
)
mutated = list(selectors)
mutated[3] += 1
mutated = tuple(mutated)


def canonical(fields):
    return struct.pack("<8s10I", b"APXLPRF1", *fields)


def as_u8(data):
    return (U8 * len(data)).from_buffer_copy(data)


count = 0


def check(name, actual, expected):
    global count
    count += 1
    if actual != expected:
        raise AssertionError(f"{name}: actual={actual!r} expected={expected!r}")


ten_u32 = [U32] * 10
dll.profile_validate_v1.argtypes = ten_u32
dll.profile_validate_v1.restype = I32
dll.profile_bootstrap_status_v1.argtypes = ten_u32
dll.profile_bootstrap_status_v1.restype = I32
dll.profile_bootstrap_manifest_status_v1.argtypes = [P8, I64] + ten_u32 + [I64] * 6
dll.profile_bootstrap_manifest_status_v1.restype = I32
dll.profile_canonical_byte_v1.argtypes = [I64] + ten_u32
dll.profile_canonical_byte_v1.restype = U32
dll.profile_id_word_v1.argtypes = [I64] + ten_u32
dll.profile_id_word_v1.restype = U32
dll.profile_id_byte_v1.argtypes = [I64] + ten_u32
dll.profile_id_byte_v1.restype = I64
dll.profile_digest_length_status_v1.argtypes = [I64]
dll.profile_digest_length_status_v1.restype = I32
dll.profile_bytes_status_v1.argtypes = [P8, I64] + ten_u32
dll.profile_bytes_status_v1.restype = I32
dll.profile_resume_status_v1.argtypes = [P8, I64] + ten_u32
dll.profile_resume_status_v1.restype = I32
dll.u128_add_lo_v1.argtypes = [U64, U64]
dll.u128_add_lo_v1.restype = U64
dll.u128_add_hi_v1.argtypes = [U64, U64, U64, U64]
dll.u128_add_hi_v1.restype = U64
dll.u128_add_status_v1.argtypes = [U64, U64, U64, U64]
dll.u128_add_status_v1.restype = I32
dll.u128_le_byte_v1.argtypes = [U64, U64, I64]
dll.u128_le_byte_v1.restype = I64
dll.profile_record_native_status_v1.argtypes = [U32, U32]
dll.profile_record_native_status_v1.restype = I32
dll.profile_variant_native_status_v1.argtypes = []
dll.profile_variant_native_status_v1.restype = I32
dll.profile_record_wire_byte_v1.argtypes = [U32, U32, I64]
dll.profile_record_wire_byte_v1.restype = U32
dll.profile_variant_wire_byte_v1.argtypes = [U32, I64]
dll.profile_variant_wire_byte_v1.restype = U32

probe_names = (
    "profile_probe_core_v1",
    "profile_probe_u128_composite_v1",
    "profile_probe_native_u128_scalar_v1",
    "profile_probe_codec_shift_v1",
    "profile_probe_unsigned_shift_semantics_v1",
    "profile_probe_u64_cross_call_v1",
    "profile_probe_record_v1",
    "profile_probe_variant_v1",
    "profile_probe_known_refusals_v1",
    "profile_probe_all_v1",
)
for name in probe_names:
    function = getattr(dll, name)
    function.argtypes = []
    function.restype = I32

current_bytes = canonical(selectors)
mutated_bytes = canonical(mutated)
current_id = hashlib.sha256(current_bytes).digest()
mutated_id = hashlib.sha256(mutated_bytes).digest()
check(
    "current bytes hex",
    current_bytes.hex(),
    "4150584c5052463102000300000002000000030000000100010000000100000000000000000000000100000000000100",
)
check(
    "current id hex",
    current_id.hex(),
    "3cbe4c61a6c3b73dc21ed3c806b4de69cb8f4b3e830729ba4cf030a28dcb1328",
)
check(
    "mutated bytes hex",
    mutated_bytes.hex(),
    "4150584c5052463102000300000002000000030001000100010000000100000000000000000000000100000000000100",
)
check(
    "mutated id hex",
    mutated_id.hex(),
    "fbd9004f62ea578aedad488e7c8f98cd713ba75261c790533e36f2a33499577a",
)

check("validate current", dll.profile_validate_v1(*selectors), 0)
check("bootstrap current refusal", dll.profile_bootstrap_status_v1(*selectors), 7001)
for index, expected in enumerate(range(101, 111)):
    changed = list(selectors)
    changed[index] ^= 1
    check(f"validate selector {index}", dll.profile_validate_v1(*changed), expected)

for fields, encoded, digest, label in (
    (selectors, current_bytes, current_id, "current"),
    (mutated, mutated_bytes, mutated_id, "mutated"),
):
    for index, value in enumerate(encoded):
        check(
            f"{label} canonical byte {index}",
            dll.profile_canonical_byte_v1(index, *fields),
            value,
        )
    for index, value in enumerate(digest):
        check(
            f"{label} id byte {index}",
            dll.profile_id_byte_v1(index, *fields),
            value,
        )
    for index in range(8):
        check(
            f"{label} id word {index}",
            dll.profile_id_word_v1(index, *fields),
            int.from_bytes(digest[index * 4 : index * 4 + 4], "big"),
        )

check("canonical low bound", dll.profile_canonical_byte_v1(-1, *selectors), 0xFFFFFFFF)
check("canonical high bound", dll.profile_canonical_byte_v1(48, *selectors), 0xFFFFFFFF)
check("id byte low bound", dll.profile_id_byte_v1(-1, *selectors), -1)
check("id byte high bound", dll.profile_id_byte_v1(32, *selectors), -1)
check("id word low bound", dll.profile_id_word_v1(-1, *selectors), 0)
check("id word high bound", dll.profile_id_word_v1(8, *selectors), 0)
check("digest length 32", dll.profile_digest_length_status_v1(32), 0)
check("digest length 31", dll.profile_digest_length_status_v1(31), 2001)

current_array = as_u8(current_bytes)
mutated_array = as_u8(mutated_bytes)
current_id_array = as_u8(current_id)
mutated_id_array = as_u8(mutated_id)
check("bytes current", dll.profile_bytes_status_v1(current_array, 48, *selectors), 0)
check("bytes short", dll.profile_bytes_status_v1(current_array, 47, *selectors), 2002)
check("bytes mismatch", dll.profile_bytes_status_v1(mutated_array, 48, *selectors), 2003)
check(
    "bytes selector composition",
    dll.profile_bytes_status_v1(mutated_array, 48, *mutated),
    104,
)
check("resume current", dll.profile_resume_status_v1(current_id_array, 32, *selectors), 0)
check("resume short", dll.profile_resume_status_v1(current_id_array, 31, *selectors), 2001)
check("resume stale codec", dll.profile_resume_status_v1(current_id_array, 32, *mutated), 3002)
check(
    "resume self-consistent codec refusal",
    dll.profile_resume_status_v1(mutated_id_array, 32, *mutated),
    104,
)
for selector_index, expected in ((6, 107), (7, 108)):
    changed = list(selectors)
    changed[selector_index] = 1
    changed = tuple(changed)
    changed_id_array = as_u8(hashlib.sha256(canonical(changed)).digest())
    check(
        f"resume self-consistent refusal {selector_index}",
        dll.profile_resume_status_v1(changed_id_array, 32, *changed),
        expected,
    )

manifest_counts = [32] * 6
check(
    "manifest current closed",
    dll.profile_bootstrap_manifest_status_v1(
        current_id_array, 32, *selectors, *manifest_counts
    ),
    7001,
)
bad_id = bytearray(current_id)
bad_id[17] ^= 0x80
check(
    "manifest content mismatch",
    dll.profile_bootstrap_manifest_status_v1(as_u8(bad_id), 32, *selectors, *manifest_counts),
    7108,
)
for index, expected in enumerate(range(7101, 7108)):
    profile_id_count = 31 if index == 0 else 32
    counts = manifest_counts.copy()
    if index > 0:
        counts[index - 1] = 31
    check(
        f"manifest count {index}",
        dll.profile_bootstrap_manifest_status_v1(
            current_id_array, profile_id_count, *selectors, *counts
        ),
        expected,
    )
changed = list(selectors)
changed[6] = 1
changed = tuple(changed)
check(
    "manifest selector composition",
    dll.profile_bootstrap_manifest_status_v1(
        as_u8(hashlib.sha256(canonical(changed)).digest()),
        32,
        *changed,
        *manifest_counts,
    ),
    107,
)

check(
    "record native admitted",
    dll.profile_record_native_status_v1(0x11223344, 0xAABBCCDD),
    0,
)
check("variant native admitted", dll.profile_variant_native_status_v1(), 0)
for index, value in enumerate(bytes.fromhex("44332211ddccbbaa")):
    check(
        f"record wire {index}",
        dll.profile_record_wire_byte_v1(0x11223344, 0xAABBCCDD, index),
        value,
    )
check("record wire low bound", dll.profile_record_wire_byte_v1(0, 0, -1), 0xFFFFFFFF)
check("record wire high bound", dll.profile_record_wire_byte_v1(0, 0, 8), 0xFFFFFFFF)
for select, expected in ((0, (0, 0, 0, 0)), (1, (1, 0, 0, 0))):
    for index, value in enumerate(expected):
        check(
            f"variant wire {select}:{index}",
            dll.profile_variant_wire_byte_v1(select, index),
            value,
        )
check("variant invalid selector", dll.profile_variant_wire_byte_v1(2, 0), 0xFFFFFFFF)
check("variant high bound", dll.profile_variant_wire_byte_v1(0, 4), 0xFFFFFFFF)

probe_expected = {name: 0 for name in probe_names}
probe_actual = {}
for name, expected in probe_expected.items():
    probe_actual[name] = getattr(dll, name)()
    check(name, probe_actual[name], expected)

rng = random.Random(0xA90C2026)
codec_cases = 64
for case in range(codec_cases):
    fields = tuple(rng.getrandbits(32) for _ in range(10))
    encoded = canonical(fields)
    digest = hashlib.sha256(encoded).digest()
    for index, value in enumerate(encoded):
        check(
            f"fuzz codec {case}:{index}",
            dll.profile_canonical_byte_v1(index, *fields),
            value,
        )
    for index, value in enumerate(digest):
        check(
            f"fuzz sha byte {case}:{index}",
            dll.profile_id_byte_v1(index, *fields),
            value,
        )
    for index in range(8):
        check(
            f"fuzz sha word {case}:{index}",
            dll.profile_id_word_v1(index, *fields),
            int.from_bytes(digest[index * 4 : index * 4 + 4], "big"),
        )

u128_cases = 4096
mask64 = (1 << 64) - 1
mask128 = (1 << 128) - 1
for case in range(u128_cases):
    left = rng.getrandbits(128)
    right = rng.getrandbits(128)
    left_lo, left_hi = left & mask64, left >> 64
    right_lo, right_hi = right & mask64, right >> 64
    total = left + right
    wrapped = total & mask128
    check(f"u128 lo {case}", dll.u128_add_lo_v1(left_lo, right_lo), wrapped & mask64)
    check(
        f"u128 hi {case}",
        dll.u128_add_hi_v1(left_lo, left_hi, right_lo, right_hi),
        wrapped >> 64,
    )
    check(
        f"u128 status {case}",
        dll.u128_add_status_v1(left_lo, left_hi, right_lo, right_hi),
        4001 if total > mask128 else 0,
    )
    byte_index = case & 15
    check(
        f"u128 le {case}",
        dll.u128_le_byte_v1(left_lo, left_hi, byte_index),
        (left >> (8 * byte_index)) & 0xFF,
    )

print(
    json.dumps(
        {
            "status": "PASS",
            "assertions": count,
            "codec_cases": codec_cases,
            "u128_cases": u128_cases,
            "current_profile_id": current_id.hex(),
            "mutated_profile_id": mutated_id.hex(),
            "probes": probe_actual,
        },
        sort_keys=True,
    )
)
