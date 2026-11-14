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
KERNEL_ELF="$ROOT/target/x86_64-unknown-none/kernel/shivacore"
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

echo "== 1/5 Kernel-ELF bauen =="
# Das Profil `kernel` erbt `release` und setzt `panic = "abort"`: das Image
# entsteht ohne Entfaltung. Die Strategie steht im Root-Manifest, nicht hier,
# damit sie nicht vom Aufrufer abhaengt.
(cd "$ROOT" && cargo +nightly build -p shivacore --profile kernel \
  --target x86_64-unknown-none --features x86-boot)

echo "== 2/5 Boot-Image-Builder bauen =="
# Der Boot-Builder ist ein eigenes Crate (Nightly-Zwang des bootloader-Crates)
# und wird deshalb in seinem Verzeichnis gebaut, nicht im Root-Workspace.
(cd "$ROOT/modules/atc-shivacore/boot" && RUSTUP_TOOLCHAIN=nightly cargo +nightly build --release)

echo "== 3/5 Init- und Anmelde-Programm bauen (Ramdisk-Nutzlast) =="
# WICHTIG: ohne RUSTFLAGS. Die Crate-Konfiguration setzt -Tlinker.ld (feste
# Ladeadresse 0x400000 fuer Init, 0x1C00000 fuer das Anmeldefenster) und
# relocation-model=static; eine RUSTFLAGS-Variable wuerde genau diese
# Einstellungen ersetzen und das ELF an eine Stelle legen, wo der Kernel es
# nicht abbildet.
( cd "$ROOT/modules/atc-shivacore/init_program" && env -u RUSTFLAGS cargo +nightly build --release )
( cd "$ROOT/modules/atc-shivacore/greeter" && env -u RUSTFLAGS cargo +nightly build --release )
INIT_ELF="$ROOT/modules/atc-shivacore/init_program/target/x86_64-unknown-none/release/shivacore-init"
GREETER_ELF="$ROOT/modules/atc-shivacore/greeter/target/x86_64-unknown-none/release/shivacore-greeter"
[ -f "$INIT_ELF" ] || die "Init-ELF fehlt: $INIT_ELF"
[ -f "$GREETER_ELF" ] || die "Anmelde-ELF fehlt: $GREETER_ELF"

echo "== 4/5 Disk-Image erzeugen =="
# Zwei Eintraege: das Init bleibt Prozess 1 (Messprogramm), das Anmeldefenster
# wird der vierte Prozess. Der Kernel waehlt nach Namen, nicht nach Reihenfolge.
"$ROOT/modules/atc-shivacore/boot/target/release/boot" "$KERNEL_ELF" "$ROOT/images" \
  "init=$INIT_ELF" "greeter=$GREETER_ELF"

echo "== 5/5 QEMU $RUN_SECONDS s (Log: $LOG) =="
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
