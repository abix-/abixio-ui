# abixio-ui changelog

## Changelog rules

These rules govern `docs/changelog.md`. Follow them exactly.

- Each day has exactly one `## YYYY-MM-DD` heading and exactly one table below
  it.
- Every daily table has exactly three columns: `System`, `Item`, and
  `Done when`.
- Start every Item with `[x]`. No preceding hyphen. Changelog rows are
  completed work only.
- Never combine completed items into one changelog row. Never replace them with
  a summary paragraph, grouped bullet, status paragraph, or count.
- When recording a completed todo row, copy its System into `System`, its
  completed instruction into `Item`, and its completion proof into `Done when`.
- Verify the exact result before adding the changelog row.
- Leave unfinished work in the todo. Do not add partial work to the changelog.
- Put new days above older days. Add rows to the existing table when that date
  already exists. Never create a second heading or second table for the same
  day.
- Use plain terms. Do not use vague descriptions, invented language, or design
  essays.

## 2026-10-21

| System | Item | Done when |
|---|---|---|
| Cargo.lock | [x] Apply the eMudhra certificate distrust fixes and redeploy | webpki-roots 1.0.10 and rustls-webpki 0.103.16; the obsolete AWS rustls 0.21 dependency is removed. Both release certificate_distrust tests pass for HTTP and AWS SDK clients against direct and cross-signed public chains, with a valid HTTPS control. The release abixio-ui executable rebuilt at its configured path. |

