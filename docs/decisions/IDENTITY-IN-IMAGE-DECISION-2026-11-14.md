# Entscheidung: Identitaetspruefung im Image (14.11.2026)

## Entscheidung des Owners

| Nr. | Entscheidung | Festlegung |
| --- | --- | --- |
| 1 | KDF-Parameter | **Uebernehmen** — Argon2id, 19 MiB, t = 2, p = 1, Parameter im Datensatz versioniert |
| 2 | Herkunft des Datensatzes | **Ramdisk** — eigene Datei, keine einkompilierten Zugangsdaten, strikte Rechte, fail-closed |
| 3 | Enter bei Erfolg | **Nur bestaetigen** — "Pruefung bestanden - Sitzung folgt.", keine vorgetaeuschte Sitzung, keine PID vor der Prozess-ABI |

Reihenfolge der ABI-Seite (Owner): `0x0060` registrieren, `SPAWN` als Recht auf
`Process`, PID eindeutig und kollisionsfrei, Start ueber den vorhandenen
`build_space`-Pfad, Uebergabe an den Scheduler, Tests fuer unberechtigten SPAWN,
ungueltige PID, fehlgeschlagenen Aufbau und erfolgreichen Start. `0x0050` bleibt
ausschliesslich der Konsole.

## Was umgesetzt ist: der gemeinsame Pruefkern

`system/identity-core` (`globus-identity-core`) ist `no_std` + `alloc` und wird
von Host und Image gemeinsam benutzt — ein Verifizierer, kein zweiter. Er baut
auch fuer `x86_64-unknown-none`, ist also im Image verwendbar.

- Datensatz: 64 Byte, Kennung `ATID`, Fassung 1, Verfahren 1 (Argon2id),
  Speicher (KiB, u16), Durchgaenge (u32), Parallelitaet, Reserviertfeld,
  Salz 16 Byte, Hash 32 Byte.
- `CredentialRecord::parse` ist fail-closed in fester Reihenfolge: Laenge,
  Kennung, Fassung, Verfahren, Reserviertfeld, Wertebereiche, Geheimnisse.
  Unbekannte Fassung wird nicht gedeutet, ein abgeschnittener Datensatz ist
  kein Datensatz, leeres Salz oder Hash wird abgelehnt.
- `verify` rechnet mit den Parametern **aus dem Datensatz** (nicht aus dem
  Programm), vergleicht ohne Fruehausstieg und meldet `KdfUnavailable`, wenn die
  KDF nicht rechnen kann.
- Grenzen: Speicher 8..32 MiB (Vielfache von 1 MiB), Durchgaenge 1..10,
  Parallelitaet 1..4. Ein Datensatz, der mehr verlangt, wird abgelehnt statt
  gerechnet.
- 9 Tests: richtiges und falsches Passwort, fehlender/abgeschnittener/
  aufgefuellter Datensatz, fremde Kennung/Fassung/Verfahren, manipulierte
  Parameter, leeres Salz oder Hash, Salt wirkt, schwaechere erlaubte Parameter.

## Messung im Image (erster Schritt) und was sie blockiert

Der Owner verlangt die Messung im Image, nicht die Uebernahme der Host-Werte.
Der erste Schritt ist deshalb die Speicherfrage: Argon2id mit 19 MiB braucht
19 MiB am Stueck.

Gemessen im Kernel-Image (QEMU/BIOS, `-m 256M`):

```
IDENT-001 Haufen: HEAP_START=0x444444440000 HEAP_SIZE=512 KiB,
Allokator meldet frei=524288 Byte, Probe-Zeiger=0x0
IDENT-001 Messung: 19 MiB NICHT verfuegbar (Haufen 512 KiB) —
die Pruefung wuerde fail-closed ablehnen
```

Das ist der ehrliche Stand: **die Pruefung kann im Image heute nicht rechnen.**
Der Kernel-Haufen ist 512 KiB (begruendet aus acht Dienst-Startabbildern zu
64 KiB); 19 MiB passen nicht hinein, und der Allokator gibt folgerichtig null
zurueck. Die Anmeldung wuerde fail-closed abweisen, nicht abstuerzen.

Wichtig fuer die Methode: der erste Messwert dieses Schritts meldete
"19 MiB verfuegbar" und war falsch. Aufgefallen ist es erst, als der freie
Speicher mitgelesen wurde (512 KiB). Eine Allokation "geglueckt" zu nennen,
weil ein Zeiger ungleich null zurueckkommt, ist kein Nachweis — der Nachweis
braucht zwei unabhaengige Werte, deshalb liest die Messung jetzt beides.

Die Zeitmessung im Image ist damit noch nicht moeglich. Sie braucht zuerst
einen 19-MiB-Bereich fuer die KDF; der naechste Schritt ist ein eigener,
temporaerer Bereich aus dem Frame-Vorrat (nicht der Haufen), mit Freigabe nach
der Rechnung. Erst danach ist "19 MiB, t=2 im Image = X ms" eine Tatsache.

## Was ausdruecklich NICHT passiert ist

- Keine Sitzung, keine PID, kein Scheduler-Uebergang in der Anmeldung
  (Entscheidung 3).
- Kein einkompiliertes Passwort und kein einkompilierter Hash (Entscheidung 2);
  der Datensatz ist ein Dateiformat, die Ramdisk-Ablage folgt mit dem Zugriff.
- Keine Uebernahme der Host-Laufzeit (24,7 ms) als Image-Nachweis.


---

# Identitaetspruefung im Image — Entscheidungsvorlage

**Datum:** 14. November 2026
**Status:** OFFEN — braucht eine Owner-Entscheidung, bevor Code entsteht
**Betroffene Repos:** globus-os (Kernel, Greeter, `system/identity`)
**Vorgaenger:** `docs/LOGIN_WINDOW.md` (Fenster und Supervisor), `modules/atc-shivacore/docs/SPAWN-ANNAHME-2026-11-09.md`

## Wo es steht

Das Anmeldefenster ist bedienbar: Maske, Tastatur ueber den echten PS/2-Pfad,
maskiertes Passwort, Feldwechsel, `Enter`-Zustand, `Esc` als Dienst-Neustart
ueber den Supervisor. Was es **nicht** tut: pruefen. Der `Enter`-Zustand nennt
den Grund im Klartext statt einen Erfolg vorzutaeuschen.

Der Grund ist keine Luecke im Fenster, sondern ein fehlendes Stueck Architektur:
die Pruefung liegt heute in `system/identity` (`globus-identity`) und ist
Host-Code — `Argon2::default()`, `bip39` mit Zufallsquelle, `file_store` fuer die
Ablage. Im Image gibt es weder Dateisystem noch Zufallsquelle noch Haufen.

## Was gemessen ist (Host, dieser Rechner)

`system/identity/src/service.rs` benutzt `Argon2::default()` = Argon2id, v19,
`m = 19456 KiB`, `t = 2`, `p = 1`.

| Profil | Parameter | Host-Zeit |
|---|---|---|
| heute im Host | m=19456 KiB, t=2, p=1 | 24,7 ms (zweiter Lauf 23,3 ms) |
| Kandidat fuer das Image | m=4096 KiB, t=3, p=1 | 6,7 ms |
| Untergrenze | m=1024 KiB, t=3, p=1 | 0,9 ms |

Nicht gemessen und deshalb **nicht** als Tatsache verwendet: die Laufzeit im
Image. Die Laeufe sind Software-Emulation ohne KVM; ein Faktor von 10 bis 25
waere eine Schaetzung, keine Messung. Fuer die Entscheidung ist die Zeit auch
nicht der Engpass (0,25–0,6 s bei 19 MiB waeren vertretbar), sondern der
Speicher: der Puffer ist `m` gross und muss im Ring 3 les- und schreibbar
abgebildet sein.

## Die drei Wege

### A — Pruefung im Fenster, gemeinsamer Kern im Ring 3

Ein eigenes Crate `globus-identity-core` (no_std, `alloc`) enthaelt genau eine
Funktion `verify(record, kennwort) -> Ergebnis` und den Verifier-Datensatz. Host
und Fenster rufen dieselbe Funktion; die Host-Seite wird auf sie umgestellt,
damit es keine zweite Wahrheit ueber das Passwort gibt. Der Datensatz kommt als
eigener Ramdisk-Eintrag ins Image, erzeugt beim Bauen aus der Host-Ablage.

- Kosten: Fenster braucht einen Haufen und ein `m` grosses RW-Abbild (bei
  19 MiB: 20 MiB Adressraum, 256 MiB hat das Image).
- Grenze: im Image ist die Ablage nur lesbar. Ein Kennwortwechsel im Image ist
  nicht moeglich, ohne Schreibspeicher.
- Risiko: das Klartext-Kennwort liegt im Prozess, solange geprueft wird; der
  Puffer muss danach genullt werden (heute zaehlt das Fenster nur die Laenge).

### B — Identitaetsdienst als eigener Ring-3-Prozess

`globus-identity` wird als Prozess portiert, das Fenster fragt per IPC
(`Syscall::IpcSend`/`IpcReceive`, Kanal wie bei der Tastatur). Ein Verifizierer,
zwei Prozesse, klare Rollen; das Fenster sieht den Verifier-Datensatz nie.

- Kosten: ein Prozess mehr, ein IPC-Vertrag (Anfrage/Antwort, Laengen, Fehler),
  und `globus-identity` muss ohne Dateisystem und ohne Zufallsquelle bauen.
- Vorteil: passt zur spaeteren Kontenverwaltung; das Fenster bleibt duenn.
- Nachteil: groesserer erster Schritt; ohne Ablage im Image bleibt es bei
  einem einzigen Konto.

### C — Pruefung im Kernel

Abgelehnt, und zwar aus zwei Gruenden, die nicht verhandelbar sind: der Kernel
bekaeme das Klartext-Kennwort zu sehen, und er muesste `m` Byte Hash-Arbeit im
Trap-Kontext leisten. Das verletzt die Trennung, die das Fenster gerade
einhaelt (Kernel orchestriert, er interpretiert nicht).

## Empfehlung

**A jetzt, B als Ziel.** Beide teilen `globus-identity-core`; der Umstieg auf B
ist dann ein Umzug des Aufrufers, kein zweiter Verifizierer. A braucht drei
Entscheidungen, die ich nicht selbst treffen kann:

1. **KDF-Profil im Image.** Gleiche Parameter wie im Host (19 MiB) ist die
   einzige Variante ohne Sicherheitsabstrich; 4 MiB waere ein bewusster
   Abstrich und muss als solcher entschieden werden, nicht als Standard
   durchrutschen.
2. **Wo der Datensatz herkommt.** Ramdisk-Eintrag aus der Host-Ablage beim
   Bauen (Empfehlung) oder fest im Programm eingebettet. Eingebettet hiesse:
   neues Kennwort = neues Programm.
3. **Was `Enter` bei Erfolg ausloest.** Heute gibt es keine Sitzung: das Fenster
   koennte nur "geprueft" anzeigen. Eine Sitzung braucht einen Prozessstart
   (offen, siehe unten) oder wenigstens einen Kanal, auf dem es weitergeht.

## Abnahmekriterien fuer A

- `globus-identity-core` ist die einzige Stelle, die einen Verifier prueft; der
  Host ruft sie auf, ein Test weist nach, dass Host und Fenster bei gleichem
  Datensatz und gleichem Kennwort dasselbe Ergebnis liefern (gleicher
  PHC-String, gleiche Parameter).
- Falsches Kennwort und unbekannter Benutzer ergeben **verschiedene** Zustaende
  im Fenster, aber dieselbe Rechenzeit-Klasse (kein "Benutzer unbekannt" vor
  der Pruefung, das waere eine Auskunft).
- Der Klartext-Puffer wird nach der Pruefung genullt; ein Test liest ihn danach.
- Byte-Laengen werden nachgezaehlt, nicht geschaetzt (Lehre aus dem
  Domaenenpraefix-Fehler in a-townchain: 15 statt 16 Byte).
- Ohne gueltigen Datensatz startet das Fenster, meldet die fehlende Ablage und
  behauptet keinen Erfolg.

## Was davon unabhaengig offen bleibt

Ein echter Prozessstart aus dem Abbild (Frame-Allokator im Syscall-Pfad,
Syscall-Nummer nicht `0x0050` — die ist durch `Console` belegt). Ohne ihn kann
`Enter` keine Sitzung beginnen; das ist die Grenze, die A nicht loest.

`CI_VERIFIED` bleibt fuer alle Punkte offen: die Workflows starten seit dem
3.11.2026 nicht (`startup_failure`), das ist eine Owner-Aktion in der
GitHub-Oberflaeche.
