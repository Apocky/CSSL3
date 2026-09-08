#include <stdint.h>

#if UINT32_MAX != 0xffffffffU
#error "ABI9001 boundary oracle requires an exact 32-bit uint32_t"
#endif

typedef struct ProfileRecordSentinel {
    uint32_t tag;
    uint32_t value;
} ProfileRecordSentinel;

extern ProfileRecordSentinel abi9001_record_identity_v1(
    ProfileRecordSentinel record
);

static int check_identity(uint32_t tag, uint32_t value) {
    const ProfileRecordSentinel input = {tag, value};
    const ProfileRecordSentinel output = abi9001_record_identity_v1(input);
    if (output.tag != tag) {
        return 1;
    }
    if (output.value != value) {
        return 2;
    }
    return 0;
}

__declspec(dllexport) int abi9001_record_c_boundary_status_v1(void) {
    const uint32_t vectors[][2] = {
        {0U, 0U},
        {0x11223344U, 0xaabbccddU},
        {0xffffffffU, 0U},
        {0U, 0xffffffffU},
        {0xffffffffU, 0xffffffffU},
    };

    for (unsigned i = 0; i < sizeof(vectors) / sizeof(vectors[0]); ++i) {
        const int status = check_identity(vectors[i][0], vectors[i][1]);
        if (status != 0) {
            return 900100 + (int)(i * 10) + status;
        }
    }

    uint32_t tag = 0x243f6a88U;
    uint32_t value = 0x85a308d3U;
    for (unsigned i = 0; i < 4096; ++i) {
        tag = tag * 1664525U + 1013904223U;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        const int status = check_identity(tag, value);
        if (status != 0) {
            return 901000 + status;
        }
    }
    return 0;
}
