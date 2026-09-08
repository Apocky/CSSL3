#include <stdint.h>

#if UINT64_MAX != 0xffffffffffffffffULL
#error "ABI9003 boundary oracle requires an exact 64-bit uint64_t"
#endif

extern uint64_t profile_u128_identity_lo_v1(
    uint64_t lo,
    uint64_t hi
);
extern uint64_t profile_u128_identity_hi_v1(
    uint64_t lo,
    uint64_t hi
);

static int check_identity(uint64_t lo, uint64_t hi) {
    if (profile_u128_identity_lo_v1(lo, hi) != lo) {
        return 1;
    }
    if (profile_u128_identity_hi_v1(lo, hi) != hi) {
        return 2;
    }
    return 0;
}

__declspec(dllexport) int abi9003_u128_c_boundary_status_v1(void) {
    const uint64_t vectors[][2] = {
        {0ULL, 0ULL},
        {0x0123456789abcdefULL, 0ULL},
        {0ULL, 0xfedcba9876543210ULL},
        {0x0123456789abcdefULL, 0xfedcba9876543210ULL},
        {~0ULL, ~0ULL},
    };

    for (unsigned i = 0; i < sizeof(vectors) / sizeof(vectors[0]); ++i) {
        const int status = check_identity(vectors[i][0], vectors[i][1]);
        if (status != 0) {
            return 900300 + (int)(i * 10) + status;
        }
    }

    uint64_t lo = 0x243f6a8885a308d3ULL;
    uint64_t hi = 0x13198a2e03707344ULL;
    for (unsigned i = 0; i < 4096; ++i) {
        lo = lo * 6364136223846793005ULL + 1442695040888963407ULL;
        hi ^= hi << 13;
        hi ^= hi >> 7;
        hi ^= hi << 17;
        const int status = check_identity(lo, hi);
        if (status != 0) {
            return 901000 + (int)status;
        }
    }
    return 0;
}
