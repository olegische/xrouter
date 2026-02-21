# Formal Models

This directory contains the current formal model for xrouter:

- `formal/xrouter.tla` — TLA+ spec (post-paid billing + streaming lifecycle)
- `formal/xrouter.cfg` — TLC config (constants + invariants + temporal properties)
- `formal/property-map.md` — property traceability
- `formal/trace-schema.md` — event/action mapping
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
