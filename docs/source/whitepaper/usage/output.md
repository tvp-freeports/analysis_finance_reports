# Output

`--out` / `-o` says where. What lands there depends on the **profile**, and two independent flags
modify it. All three are settable from any configuration source, not only from the command line —
{doc}`configuration/options`.

## The three profiles

`regular`
: the default. A directory, one CSV per table, plus `investments_add_infos.yaml` for the details
  that belong only to bonds.

`single_file`
: the path is a *file*, not a directory: one CSV with the investment columns plus `Maturity` and
  `Interest rate` folded in. Only investments are written; the other tables have nowhere to go. If
  the path does not end in `.csv`, `out.csv` is appended to it.

`structured`
: a directory holding `investments/table.csv` and `investments/dicts.yaml`. Only investments, for
  the same reason.

Two of the three write only investments. That is not a limitation of those profiles so much as what
they are *for*: they exist for consumers who want one artefact per run, and a fund table has nowhere
to go in one file without inventing a join.

## The tables of the regular profile

| File | Holds |
|---|---|
| `investments.csv` | one row per holding |
| `investments_add_infos.yaml` | maturity and interest rate, keyed by investment |
| `funds.csv` | the funds encountered |
| `funds_assets.csv` | total assets, liabilities, net assets |
| `funds_sfdr_classification.csv` | SFDR article per fund |
| `funds_esg_indicators.csv` | ESG indicators per fund |
| `assets_managers.csv` | management companies |
| `investments_managers_to_funds.csv` | which manager runs which fund |
| `funds_change_name.csv` | renamings and mergers |

Plus `.log.csv`, which is not a table — see {doc}`logging`.

## Three rules across every profile

They exist so a consumer never has to guess, and they are invariants of the *product*: an output
file that violates one is wrong even if every step that produced it was right
({doc}`../design/entities-and-output`).

- **every CSV always has its header**, even with no rows. A table with nothing in it is a
  header-only file, never a missing one, so "no rows" and "no file" stay distinguishable;
- **headers are exact**, in text and in order;
- **an absent optional value is an empty cell** — never `None`, never `null` — and a floating-point
  number always carries at least one decimal.

## The two flags

### `--archive` / `-z`

Additionally writes a compressed **sibling** of the output: `.tar.gz` for the directory profiles,
`.gz` for `single_file`.

Whether the uncompressed output is then removed depends on whether it **already existed before the
run**, checked before anything is written: a directory you already had is never deleted. A path
given as `-o results.tar.gz` switches the flag on by itself and the suffix is stripped, so it means
the same as `-o results -z`.

### `--separate-out`

Splits the two tables that carry a report and a format per row — `investments` and `funds_assets` —
into one file per `(report, format)` pair:

```text
investments__EURIZON 2023__EURIZON-EN23.csv
funds_assets__EURIZON 2023__EURIZON-EN23.csv
```

The other tables stay merged, because they do not carry those two columns and splitting them would
mean inventing an attribution.

```{warning}
The document's **name** becomes part of the file name. A document left unnamed defaults to its whole
absolute path, and a path contains separators, so `--separate-out` on an unnamed document tries to
write a file whose name is a path and fails. Name your documents — `-i report.pdf:EURIZON 2023` —
whenever this flag is on. See {doc}`documents`.
```

## One run, one set of tables

Results from every page, every document and every job of the run accumulate and are written
**once**, at the end. Two documents in one invocation share promise resolution and fund
deduplication; the same two documents in two invocations do not, and concatenating the results
afterwards is not equivalent. {doc}`../design/multidocument`.
