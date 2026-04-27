<!--
Thanks for sending a PR. Quick checklist before requesting review:
-->

## Summary

<!-- 1-2 sentences. What changes, and why. -->

## Tests

<!-- Per CLAUDE.md, Rust changes need a failing-test-first commit. Link or quote the new test. -->

- [ ] New tests cover the change (Rust integration / unit / wire-format / Vitest)
- [ ] `./errex.sh check` is green locally (fmt + clippy + cargo test + bun test)
- [ ] No new deps without a justification in the description

## Notes

<!--
Anything reviewer should know: tradeoffs, follow-ups, what's deliberately not in scope.
If this changes the wire format, list which fields were added / removed / renamed.
-->
