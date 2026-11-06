# SC-006 — ACK-Design fuer Geraete-Abonnenten

Stand 07.11.2026, umgesetzt und im Bootlauf geprueft.

## Die Frage hinter dem ACK

REQ-SC006-03 verlangt "maskiert bis ACK". Das beschreibt den Zustand, nicht den
Weg: es sagt nicht, wer quittiert, wann, und was passiert, wenn niemand
quittiert. Ohne diese drei Antworten ist die Anforderung entweder nicht
erfuellbar (kein ACK-Pfad) oder gefaehrlich (eine Line bleibt fuer immer
gesperrt).

## Entscheidung

**Quittung beim Empfang, Frist als Netz, Aussetzung nach drei Fehlversuchen.**

1. **Der Empfang ist die Quittung.** Holt der Abonnent die Nachricht ab oder
   verarbeitet sie, quittiert er damit die Zustellung (`ack` /
   `ack_on_receive`). Ein eigener ACK-Syscall waere eine ABI-Erweiterung ohne
   neuen Nutzen: er koennte nur sagen, was der Empfang schon sagt.
2. **Die Frist ist die Sicherung.** Bleibt die Quittung laenger als
   `ACK_DEADLINE_TICKS` (8 Ticks) aus, gibt der Kernel die Line selbst frei
   (`IrqTable::reclaim`), zaehlt die verpasste Quittung und sperrt nach
   `ACK_STRIKE_LIMIT` (3) verpassten Quittungen das Abonnement.
3. **Wiederaufnahme ist ausdruecklich.** Ein ausgesetztes Abonnement laeuft nur
   nach `resume` mit der IRQ-Cap wieder an. Stille Wiederaufnahme waere ein
   Kreis, in dem sich ein Prozess selbst beliebig oft zurueckholt.

Die Frist weicht bewusst nicht von REQ-03 ab: die Line ist weiterhin bis zur
Quittung maskiert. Sie verhindert nur, dass "bis zur Quittung" zu "fuer immer"
wird.

## Warum nicht nur Frist (Weg 3 des Entwurfs)

Reine Fristverwaltung macht jeden Trigger zu einem Wettlauf: ein langsamer, aber
korrekter Prozess verliert seine Zustellung, obwohl er nichts falsch gemacht hat.
Der Empfang als Quittung kostet dagegen nichts und trifft genau die Absicht.

## Warum nicht nur Empfang (Weg 1 des Entwurfs)

Ohne Frist haelt ein Prozess, der nicht mehr empfaengt, die Line dauerhaft
gesperrt. Das Geraet faellt still aus: kein Fehler, keine Meldung, nur keine
Interrupts mehr. Ein Sicherheitsnetz ist billiger als die Fehlersuche.

## Zustaende einer Line

    Idle --(Trigger)--> Pending(maskiert, Frist laeuft)
    Pending --(ACK)--> Idle                       [gezaehlt: acked_on_receive]
    Pending --(Frist)--> Idle, Serie + 1          [gezaehlt: timed_out]
    Serie 3 --> ausgesetzt (keine Zustellung)     [gezaehlt: suspended]
    ausgesetzt --(resume mit IRQ-Cap)--> Idle, Serie 0

Ein Trigger waehrend Pending wird koalesziert und gezaehlt (`coalesced`), nicht
zugestellt. Ein Trigger waehrend "ausgesetzt" wird unterdrueckt und gezaehlt
(`suppressed`) — damit ist die Ursache sichtbar und nicht nur die Wirkung.

## Rechte

| Vorgang | Recht auf der IRQ-Cap |
| --- | --- |
| Abonnieren | `SUBSCRIBE` |
| Quittieren | `ACK` |
| Wiederaufnehmen | `ACK` (wer quittieren darf, darf zurueckholen) |
| Zeitablauf | keins — Kernel-Autoritaet, gezaehlt |

Hinweis aus der Praxis: `create_typed` vergibt fuer IRQ-Caps `SUBSCRIBE` und
`ACK` zusammen. Ein IRQ-Cap ohne ACK-Recht laesst sich damit gar nicht bauen;
die Rechtepruefung ist trotzdem im Code, weil die Cap-Tabelle sie zulaesst.

## Beleg

7 Tests in `irq_bridge::tests` und `irq::tests` decken ab: Empfangs-Quittung
gibt frei, Frist gibt frei und zaehlt, drei Fristen setzen aus, Aussetzung
liefert nichts mehr, Wiederaufnahme braucht die Cap, eine Quittung setzt die
Serie zurueck, und eine Quittung annulliert ihre eigene Frist.

Der letzte Punkt war ein echter Fehler: der Kernel quittiert im Handler, der
Eintrag blieb aber in der Fristliste stehen und wurde beim Ablauf
stillschweigend verworfen. Die Frist haette damit nichts zu sagen gehabt, ohne
dass es aufgefallen waere. Jede Quittung raeumt jetzt ihren Eintrag.

## Grenzen

- Ein Takt von 8 Ticks ist eine Annahme, kein Messwert. Bei sehr kurzen Ticks
  ist die Frist zu knapp; sie ist deshalb eine Konstante und keine Streuung im
  Code.
- Der Zeitablauf gibt die Line frei, ohne den Abonnenten zu benachrichtigen. Er
  erfaehrt es beim naechsten `IpcReceive` an der Zaehlung, nicht durch eine
  eigene Nachricht.
