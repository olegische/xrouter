# Formal Models

This directory contains the active and legacy formal models for xrouter:

- Active non-billing model:
  - `formal/xrouter.tla` — TLA+ spec (`ingest -> tokenize -> generate(stream) -> done|failed`)
  - `formal/xrouter.cfg` — TLC config (constants + invariants + temporal properties)
  - `formal/property-map.md` — property traceability
  - `formal/trace-schema.md` — event/action mapping
- Legacy billing model (reference for future extension work):
  - `formal/xrouter_billing.tla`
  - `formal/xrouter_billing.cfg`
  - `formal/property-map-billing.md`
  - `formal/trace-schema-billing.md`
- `formal/open-formalization-questions.md` — unresolved modeling decisions

## Requirements

- Java (JRE/JDK)
- `tlc` on `PATH`

## Run TLC

From repository root:

```bash
tlc -workers 1 -config formal/xrouter.cfg formal/xrouter.tla
```

Recommended clean run:

```bash
tlc -cleanup -workers 1 -config formal/xrouter.cfg formal/xrouter.tla
```

Notes:

- `-workers 1` is preferred for deterministic local checks.
- Do not pass `-deadlock` unless you intentionally want to disable deadlock checking.
