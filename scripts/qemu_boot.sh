#!/usr/bin/env bash
# ShivaCore QEMU-Bootlauf (BIOS-Image, headless, serielle Ausgabe).
#
# Belegt den Bootpfad ohne echte Hardware: Kernel-ELF -> Boot-Image -> QEMU.
# Aufruf: scripts/qemu_boot.sh [Sekunden] [Logpfad] [bios|uefi]
#
# Zwei Fallstricke, die dieses Skript kapselt:
#   1. `bootloader` 0.11 ruft in seinem Build-Skript intern `cargo` mit -Z auf.
#      Ohne nightly bricht der Boot-Image-Builder ab. RUSTUP_TOOLCHAIN=nightly
#      sorgt dafuer, dass auch der innere Aufruf nightly sieht.
#   2. Der Boot-Image-Builder braucht `llvm-tools-preview` (objcopy-Ersatz).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_SECONDS="${1:-20}"
LOG="${2:-/tmp/shivacore-boot.log}"
KERNEL_ELF="$ROOT/target/x86_64-unknown-none/release/shivacore"
IMAGE="$ROOT/images/shivacore-bios.img"

die() { echo "FEHLER: $*" >&2; exit 1; }

MODE="${3:-bios}"

command -v qemu-system-x86_64 >/dev/null || die "qemu-system-x86_64 fehlt (apt-get install qemu-system-x86)"
command -v cargo >/dev/null || die "cargo fehlt"
rustup +nightly component list --installed 2>/dev/null | grep -q llvm-tools \
  || die "llvm-tools fehlt: rustup +nightly component add llvm-tools-preview"

# Die bare-metal-Ziele brauchen eine explizite Target-CPU und einen definierten
# curve25519-Backend; sonst bricht die Krypto-Schicht beim Bare-Metal-Build ab.
export RUSTFLAGS='-C target-cpu=x86-64 --cfg curve25519_dalek_backend="serial"'

echo "== 1/4 Kernel-ELF bauen =="
(cd "$ROOT" && cargo +nightly build -p shivacore --release \
  --target x86_64-unknown-none --features x86-boot)

echo "== 2/4 Boot-Image-Builder bauen =="
(cd "$ROOT" && RUSTUP_TOOLCHAIN=nightly cargo +nightly build --release -p boot)

echo "== 3/4 Disk-Image erzeugen =="
"$ROOT/target/release/boot" "$KERNEL_ELF" "$ROOT/images"

echo "== 4/4 QEMU $RUN_SECONDS s (Log: $LOG) =="
set +e
case "$MODE" in
  bios)
    timeout "$RUN_SECONDS" qemu-system-x86_64 \
      -drive format=raw,file="$IMAGE" \
      -serial file:"$LOG" -display none -no-reboot -m 256M
    ;;
  uefi)
    # OVMF stellt das UEFI-Firmware-Image bereit (Paket ovmf).
    OVMF="${OVMF_CODE:-/usr/share/OVMF/OVMF_CODE.fd}"
    [ -f "$OVMF" ] || die "OVMF-Firmware fehlt: apt-get install ovmf"
    timeout "$RUN_SECONDS" qemu-system-x86_64 \
      -drive format=raw,file="$ROOT/images/shivacore-uefi.img" \
      -drive if=pflash,format=raw,readonly=on,file="$OVMF" \
      -serial file:"$LOG" -display none -no-reboot -m 256M
    ;;
  *) die "Modus '$MODE' unbekannt (bios|uefi)" ;;
esac
status=$?
set -e
# Exit 124 heisst: der Kernel lief bis zum Timeout weiter. Das ist der Normalfall.
if [ "$status" -ne 0 ] && [ "$status" -ne 124 ]; then
  die "QEMU endete mit Status $status"
fi

echo "-- Meilensteine --"
for marker in "Kernel-Einstiegspunkt erreicht" "CPU HAL compatibility check OK" \
              "ACPI RSDP/root/table checksums validated" "GDT/IDT/PIC OK" \
              "Paging-Mapper + Heap initialisiert" "KernelState::boot() -> Done" \
              "MADT local_apic=" "USER-001 entering"; do
  if grep -q "$marker" "$LOG"; then echo "  [OK] $marker"; else echo "  [FEHLT] $marker"; fi
done
echo "  Kontextwechsel: $(grep -c 'context switch' "$LOG")"
echo "  Logzeilen:      $(wc -l < "$LOG")"
