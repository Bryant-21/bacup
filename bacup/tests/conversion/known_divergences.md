# Known Parity Divergences — Legacy vs Rust Backend

This file documents accepted parity gaps between the Python legacy backend and
the Rust backend during the B.5/B.6 migration phase.

Once all gaps are resolved this file should be empty (but kept in the repo so
`test_parity_layer3.py` can import it without error).

## Format

Each accepted divergence should be listed as a bullet with the FormKey and a
brief reason:

```
- <FormKey>  # reason: <why this divergence is accepted>
```

## Accepted divergences

*(none yet — the Rust backend is in early integration; update this file as
real-game parity runs surface accepted gaps)*
