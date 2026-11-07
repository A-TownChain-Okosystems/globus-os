#!/usr/bin/env python3
# Copyright (c) 2026 Michael Wroblewski / ShivaCore / A-TownChain-Okosystems. All Rights Reserved.
"""Boot-Monitor fuer ShivaCore.

Warum es das gibt: Zwei Messfehler haben in diesem Projekt mehr Zeit gekostet
als jeder echte Kernel-Fehler.

1. Ein Log, das mitten im Lauf endet, sieht wie ein Absturz aus. Der BIOS-Lauf
   braucht aber mehr als 18 Sekunden; bei zu kurzem Zeitfenster fehlte einfach
   das Ende.
2. Ein echter Haenger sieht wie "laeuft noch" aus, weil ein laufender Boot nie
   von selbst endet. Ein Zeitlimit allein unterscheidet die beiden nicht.

Deshalb misst der Monitor **Wachstum statt Ende**: er liest das Log zweimal mit
Abstand und vergleicht. Kommt nichts mehr dazu, steht die Maschine — egal, wie
lang man wartet. Und er prueft nicht, ob irgendetwas im Log steht, sondern ob
die erwarteten Meilensteine in der richtigen Reihenfolge vorkommen.

Aufruf:
    python3 scripts/boot_monitor.py --images img-ok,img-gp
    python3 scripts/boot_monitor.py --images img-ok --firmwares bios,uefi --repeats 2

Rueckgabewert: 0 nur, wenn jeder Lauf OK ist.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time

OVMF = "/usr/share/OVMF/OVMF_CODE.fd"

# Meilensteine des Bootpfads, in der Reihenfolge, in der sie erscheinen muessen.
# Fehlt einer, ist der Boot nicht bis zur Arbeit gekommen — auch ohne Panic.
MILESTONES = [
    ("ramdisk", r"Ramdisk-Verzeichnis: \d+ Programme"),
    ("elf", r"aus Ramdisk: \d+ Bytes ELF"),
    ("ring3", r"USER-001 mapped|mapped PID"),
    ("scheduler", r"scheduler armed"),
    ("syscall", r"Syscall aus Ring 3"),
]

PANIC = re.compile(r"PANIC|panicked at|Kernelausnahme", re.IGNORECASE)
SWITCH = re.compile(r"context switch to PID=\d+")
FAULT = re.compile(r"ShivaCore: (?:Seitenfehler|Schutzverletzung|Ungueltiger Opcode) PID=(\S+)")

OK, PANIC_V, HANG, INCOMPLETE = "OK", "PANIC", "HANG", "UNVOLLSTAENDIG"


def qemu_cmd(image_dir, firmware, log):
    common = [
        "qemu-system-x86_64",
        "-serial", f"file:{log}",
        "-display", "none",
        "-no-reboot",
        "-m", "256M",
    ]
    if firmware == "bios":
        return common + ["-drive", f"format=raw,file={image_dir}/shivacore-bios.img"]
    return common + [
        "-drive", f"format=raw,file={image_dir}/shivacore-uefi.img",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF}",
    ]


def read(path):
    try:
        with open(path, "rb") as handle:
            return handle.read().decode("utf-8", errors="replace")
    except FileNotFoundError:
        return ""


def monitor_once(image_dir, firmware, seconds, log):
    """Startet einen Boot, misst zweimal und beendet ihn."""
    if os.path.exists(log):
        os.remove(log)

    start = time.monotonic()
    proc = subprocess.Popen(
        qemu_cmd(image_dir, firmware, log),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    # Erste Probe nach der Haelfte, zweite kurz vor dem Ende: zwischen beiden
    # muss das Log gewachsen sein, sonst steht die Maschine.
    time.sleep(max(1.0, seconds * 0.5))
    first = read(log)
    time.sleep(max(1.0, seconds * 0.5))
    second = read(log)

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)

    return {
        "image": image_dir,
        "firmware": firmware,
        "seconds": round(time.monotonic() - start, 1),
        "first_bytes": len(first),
        "second_bytes": len(second),
        "log": second,
    }


def judge(run):
    """Urteil aus einem Lauf. Reihenfolge zaehlt: Panic vor Haenger vor Rest."""
    log = run["log"]
    run["switches"] = len(SWITCH.findall(log))
    run["faults"] = FAULT.findall(log)
    run["missing"] = [name for name, pattern in MILESTONES if not re.search(pattern, log)]

    if PANIC.search(log):
        run["verdict"] = PANIC_V
        run["reason"] = "Panic im Log"
        return run

    grew = run["second_bytes"] > run["first_bytes"]
    if not grew:
        run["verdict"] = HANG
        run["reason"] = (
            f"Log wuchs nicht ({run['first_bytes']} -> {run['second_bytes']} Byte), "
            "Maschine steht"
        )
        return run

    if run["missing"]:
        run["verdict"] = INCOMPLETE
        run["reason"] = "Meilenstein fehlt: " + ", ".join(run["missing"])
        return run

    run["verdict"] = OK
    run["reason"] = f"{run['switches']} Kontextwechsel"
    return run


GOOD_LOG = """ShivaCore: Ramdisk-Verzeichnis: 2 Programme (init, probe)
ShivaCore: Programm 'init' aus Ramdisk: 6576 Bytes ELF, Einstieg 0x400000
USER-001 mapped PID=1000 RIP=0x400000 RSP=0x7ffff000
SCHED-001 scheduler armed: PID=1000 + PID=1001 (Ring 3)
context switch to PID=1000
Syscall aus Ring 3: Nr=1 PID=1000 Opcode=0x0001
context switch to PID=1001
"""


def selftest():
    """Prueft das Urteil gegen erfundene Logs.

    Ohne diesen Teil waere der Monitor ein Werkzeug, das immer "OK" sagt — und
    ein Werkzeug, das immer OK sagt, ist schlimmer als keins, weil man ihm
    glaubt. Jeder Fall hier ist ein Fehler, der in diesem Projekt wirklich
    vorkam.
    """
    cases = [
        (
            "gesunder Lauf",
            GOOD_LOG,
            {"first_bytes": 100, "second_bytes": 400},
            OK,
        ),
        (
            "Panic im Log",
            GOOD_LOG + "PANIC: SCHED-001 register corruption\n",
            {"first_bytes": 100, "second_bytes": 900},
            PANIC_V,
        ),
        (
            "Log endet mitten im Lauf (der 18-Sekunden-Fehler)",
            "ShivaCore: Ramdisk-Verzeichnis: 2 Programme (init, probe)\n"
            "ShivaCore: Programm 'init' aus Ramdisk: 6576 Bytes ELF\n",
            {"first_bytes": 120, "second_bytes": 120},
            HANG,
        ),
        (
            "Meilenstein fehlt, Maschine laeuft aber",
            GOOD_LOG.replace("Syscall aus Ring 3: Nr=1 PID=1000 Opcode=0x0001\n", ""),
            {"first_bytes": 100, "second_bytes": 400},
            INCOMPLETE,
        ),
    ]

    failures = 0
    for name, log, sizes, expected in cases:
        run = {"log": log, "image": "test", "firmware": "test"}
        run.update(sizes)
        verdict = judge(run)["verdict"]
        marks = "ok" if verdict == expected else f"ERWARTET {expected}"
        if verdict != expected:
            failures += 1
        print(f"  {verdict:<14} {name:<52} {marks}")

    print(f"\nSelbsttest: {len(cases) - failures} von {len(cases)} korrekt")
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description="Boot-Monitor fuer ShivaCore")
    parser.add_argument("--images", default="images", help="Bildverzeichnisse, mit Komma getrennt")
    parser.add_argument("--firmwares", default="bios,uefi")
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--seconds", type=float, default=30.0)
    parser.add_argument("--report", default=None, help="JSON-Bericht schreiben")
    parser.add_argument("--selftest", action="store_true", help="Urteil gegen erfundene Logs pruefen")
    args = parser.parse_args()

    if args.selftest:
        return selftest()

    images = [part.strip() for part in args.images.split(",") if part.strip()]
    firmwares = [part.strip() for part in args.firmwares.split(",") if part.strip()]

    for image in images:
        if not os.path.isdir(image):
            print(f"FEHLER: Bildverzeichnis fehlt: {image}", file=sys.stderr)
            return 2

    if "uefi" in firmwares and not os.path.exists(OVMF):
        print(f"FEHLER: OVMF fehlt ({OVMF}). Firmware bios verwenden.", file=sys.stderr)
        return 2

    runs = []
    for image in images:
        for firmware in firmwares:
            for attempt in range(1, args.repeats + 1):
                log = f"/tmp/shivacore-monitor-{os.path.basename(image)}-{firmware}-{attempt}.log"
                run = judge(monitor_once(image, firmware, args.seconds, log))
                run["attempt"] = attempt
                run["log_path"] = log
                runs.append(run)
                print(
                    f"{run['verdict']:<14} {os.path.basename(image):<10} {firmware:<4} "
                    f"#{attempt}  {run['reason']}"
                )

    print()
    print(f"{'Urteil':<14}{'Bild':<12}{'Firmware':<10}{'Wechsel':>8}{'Fehler':>7}{'Sekunden':>10}")
    for run in runs:
        print(
            f"{run['verdict']:<14}{os.path.basename(run['image']):<12}{run['firmware']:<10}"
            f"{run['switches']:>8}{len(run['faults']):>7}{run['seconds']:>10}"
        )

    if args.report:
        with open(args.report, "w", encoding="utf-8") as handle:
            json.dump(
                [{k: v for k, v in run.items() if k != "log"} for run in runs],
                handle,
                indent=2,
                ensure_ascii=False,
            )
        print(f"\nBericht: {args.report}")

    bad = [run for run in runs if run["verdict"] != OK]
    if bad:
        print(f"\n{len(bad)} von {len(runs)} Laeufen nicht OK", file=sys.stderr)
        return 1
    print(f"\n{len(runs)} von {len(runs)} Laeufen OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
