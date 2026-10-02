#!/usr/bin/env bash
set -euo pipefail

LOG_FILE="${1:?usage: boot-blocker.sh <qemu-log>}"

fail() {
  echo "BOOT-BLOCKER: FAIL — $1"
  exit 1
}

[ -s "$LOG_FILE" ] || fail "QEMU log is missing or empty: $LOG_FILE"

required_markers=(
  "ShivaCore: Kernel-Einstiegspunkt erreicht."
  "ShivaCore: CPU HAL compatibility check OK."
  "ShivaCore: USER-001 mapped PID=1000"
  "ShivaCore: SCHED-001 scheduler armed: PID=1000 + PID=1001"
  "ShivaCore: USER-001 entering PID=1000"
  "ShivaCore: scheduler preemption -> context switch to PID=1001"
  "ShivaCore: scheduler preemption -> context switch to PID=1000"
  "ShivaCore: SCHED-001 register checkpoint PID=1000 RAX=0x1000 RBX=0x1001 R12=0x1012"
  "ShivaCore: SCHED-001 register checkpoint PID=1001 RAX=0x2000 RBX=0x2001 R12=0x2012"
  "ShivaCore: KernelState::boot() -> Done."
)

for marker in "${required_markers[@]}"; do
  grep -Fq "$marker" "$LOG_FILE" || fail "required boot evidence missing: $marker"
done

grep -Fq "ShivaCore: KernelState::boot() failed" "$LOG_FILE" &&   fail "kernel reported canonical KernelState::boot() failure"

grep -Fq "panic" "$LOG_FILE" &&   fail "kernel panic detected in QEMU log"

grep -Fq "triple fault" "$LOG_FILE" &&   fail "triple fault detected in QEMU log"

grep -Fq "BOOT-BLOCKER: FAIL" "$LOG_FILE" &&   fail "boot log contains a prior boot-blocker failure"

echo "BOOT-BLOCKER: PASS — kernel entry, CPU HAL, Ring-3 entry, scheduler round-trip, register checkpoints, and canonical KernelState boot verified."
