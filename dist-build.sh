#!/usr/bin/env bash
# § dist-build.sh · package LoA.exe + alpha-tester-docs into a-ZIP
# § output : dist/LoA-v0.1.0-alpha-windows-x64.zip + .blake3
set -euo pipefail

VERSION="v0.1.0-alpha"
PLATFORM="windows-x64"
ARCHIVE_NAME="LoA-${VERSION}-${PLATFORM}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOA_DIR="${REPO_ROOT}/Labyrinth of Apocalypse"
DIST_DIR="${REPO_ROOT}/dist"
STAGE_DIR="${DIST_DIR}/${ARCHIVE_NAME}"

echo "§ dist-build · ${ARCHIVE_NAME}"

# 1 · ensure LoA.exe exists
if [[ ! -x "${LOA_DIR}/LoA.exe" ]]; then
  echo "ERR : LoA.exe not found · run build.sh first" >&2
  exit 1
fi

# 2 · stage
rm -rf "${STAGE_DIR}"
mkdir -p "${STAGE_DIR}"
cp "${LOA_DIR}/LoA.exe" "${STAGE_DIR}/LoA.exe"
cp "${REPO_ROOT}/dist/README.md" "${STAGE_DIR}/README.md"
cp "${REPO_ROOT}/dist/LICENSE.md" "${STAGE_DIR}/LICENSE.md"
cp "${REPO_ROOT}/dist/CONTROLS.md" "${STAGE_DIR}/CONTROLS.md"

# 3 · zip
cd "${DIST_DIR}"
rm -f "${ARCHIVE_NAME}.zip"
if command -v zip >/dev/null 2>&1; then
  zip -9 -r "${ARCHIVE_NAME}.zip" "${ARCHIVE_NAME}/" >/dev/null
else
  powershell.exe -NoProfile -Command "Compress-Archive -Path '${ARCHIVE_NAME}' -DestinationPath '${ARCHIVE_NAME}.zip' -CompressionLevel Optimal -Force" >/dev/null
fi

# 4 · BLAKE3 hash via PowerShell + .NET if blake3 cli not present
if command -v b3sum >/dev/null 2>&1; then
  b3sum "${ARCHIVE_NAME}.zip" | awk '{print $1}' > "${ARCHIVE_NAME}.zip.blake3"
elif command -v sha256sum >/dev/null 2>&1; then
  echo "(no blake3 · falling back to sha256)"
  sha256sum "${ARCHIVE_NAME}.zip" | awk '{print $1}' > "${ARCHIVE_NAME}.zip.sha256"
else
  echo "warn : no hash tool found · skipping integrity hash" >&2
fi

# 5 · summary
echo
echo "§ DIST READY"
SIZE_BYTES=$(stat -c%s "${ARCHIVE_NAME}.zip" 2>/dev/null || stat -f%z "${ARCHIVE_NAME}.zip")
SIZE_MB=$(awk "BEGIN { printf \"%.2f\", ${SIZE_BYTES} / 1048576 }")
echo "  archive : ${DIST_DIR}/${ARCHIVE_NAME}.zip"
echo "  size    : ${SIZE_MB} MB"
if [[ -f "${ARCHIVE_NAME}.zip.blake3" ]]; then
  echo "  blake3  : $(cat ${ARCHIVE_NAME}.zip.blake3)"
elif [[ -f "${ARCHIVE_NAME}.zip.sha256" ]]; then
  echo "  sha256  : $(cat ${ARCHIVE_NAME}.zip.sha256)"
fi
echo
echo "§ next : upload to downloads.apocky.com OR cssl-edge/public/"
