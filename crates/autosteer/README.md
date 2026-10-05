# nativechat-autosteer

Queue / steer / interrupt advice for a message typed while a turn is running.

- **NativeChat owns the what**: `data/lexicon.toml`, `data/rules.toml` (classes
  `[queue, steer, interrupt]`, option 0 is the safe default), `data/eval.jsonl` (221 synthetic
  labelled messages), `golden/journal.jsonl`, and the rule that an interrupt is never auto-applied
  (`AutoApply::Never`).
- **PUA owns the how**: `pua_rules::RuleClassifier` (normalize, lexicon with typo repair and
  confusable guard, rules with negation, integer scores, abstain on low confidence/margin), pinned
  by git rev in `Cargo.toml`.

Moved from `hexuria/pua` `packs/pua-steer` (see `docs/architecture-audit.md` there). The
`DataVersion` domain tag stays `pua-steer/1`, so the golden journal is byte-identical to PUA's.

```sh
cargo test -p nativechat-autosteer                       # unit, proptests, eval gate, golden replay
cargo run  -p nativechat-autosteer --example eval        # regenerates EVAL.md
cargo run  -p nativechat-autosteer --example write_golden # after a data or PUA-rev change
cargo run  -p nativechat-autosteer --example replay -- crates/autosteer/golden/journal.jsonl
```

Not wired into the composer yet: `send_policy::OnSend` has no `Auto` state. Wiring it is a
separate change; the crate is pure (no gpui) so it builds and tests on any host.
