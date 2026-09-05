# Watching a run, and reading it afterwards

Three destinations, deliberately separate, because what a person watches while a run happens and
what a tool parses afterwards are not the same artefact ({doc}`../design/limits` has the reasoning).

| Destination | Where it lands | For | Level |
|---|---|---|---|
| stderr | your terminal | watching it happen | your verbosity |
| `freeports.log.jsonl` | the **working** directory | one JSON object per line, for tools | your verbosity |
| `.log.csv` | the **output** directory | the extraction's own audit trail | `warn` and above |

## The six verbosity levels
`-v` and `-q` are **independent dials** rather than opposed flags: the net of the two counts is
added to the default and clamped, so no combination is an error.

| Flags | Level |
|---|---|
| none | warnings |
| `-q` | errors only |
| `-qq` or more | silent |
| `-v` | info |
| `-vv` | debug |
| `-vvv` or more | trace |

```{warning}
This is today the **only** way to set the verbosity of a run. `FREEPORTS_VERBOSITY` and the
configuration file's `verbosity` key are parsed, validated and merged into the resolved
configuration, but the parent process installs its logging before the configuration is resolved and
never revisits it. Curiously, worker child processes *do* honour the resolved value, because it
reaches them inside their request — so a batch run in child processes and the parent that spawned
them can disagree. See {doc}`configuration/index`.
```

## stderr

Built to be read at a glance during a run, not to be parsed. No timestamp — while you are watching,
"now" is not information — and no module path. What it does carry is the **activity**: the span path
saying what the engine was doing when the line was emitted, coloured in four shades so the structure
is visible without reading it word by word.

```text
 WARN run/job[EURIZON-EN23]/step[0]/class[investments]/document[EURIZON 2023]/page[16]/pipeline[investments]/text_filter/pipe[TextFilterInvestmentsStandard]:
      expected text block not found near the matched company - row skipped coord_ref_1="Leonardo Spa Az Nom" coord_ref_2=Leonardo coord_1="row 12" coord_2="col 1"
```

That path is the point. It says *what was happening* — job, step, page class, document, page,
pipeline, segment, pipe — rather than which source file the line came from, which is what you want
when a format misbehaves on one page of one report. `job` is the **format**; `document` is the
report, and it is what tells two documents of one run apart.

### Where a value ends

Most of what these lines carry is **text copied out of the report** — a company as the document
spells it, a fund's name — and that text contains spaces, dashes and sometimes commas. So a value
is quoted wherever something else stands beside it:

```text
investment["EXXON MOBIL CORP - 110.00 - 16.08.24 PUT","row 12","col 6"]
job[AMUNDI-EN24]
page[53]
```

A span carrying **one** field renders it bare, which is nearly all of them; a span carrying several
quotes each. In the `key=value` tail after the message the rule is the same ambiguity seen from the
other side: a value with a space in it is quoted, one without cannot run into the next key and is
left alone.

The quoting is CSV's — a `"` inside a value is doubled — and deliberately not Rust's `Debug`, which
would escape the accents out of `Société Générale` and defeat the purpose of printing text you are
meant to paste into a PDF viewer's search box.

```{note}
The colours are emitted even when stderr is not a terminal, so `2>file` collects the escape
sequences too. Piping through `sed -r 's/\x1b\[[0-9;]*m//g'` strips them.
```

## `freeports.log.jsonl`

One JSON object per line, at whatever level the verbosity allows, in the **working** directory. Each
record carries the wall clock, the level, the same activity path stderr prints, the `target` module
that stderr omits, the message, any coordinates, and — where the event attached one — the error in
structured form, with its `Debug` shape, its message, and its whole `source()` chain.

```json
{"time":"2026-08-31T09:14:02.118773Z","level":"WARN","activity":"run/job[EURIZON-EN23]/step[0]/class[investments]/document[EURIZON 2023]/page[16]/pipeline[investments]/text_filter/pipe[TextFilterInvestmentsStandard]","target":"freeports::formats_utils::text_filter","message":"expected text block not found near the matched company - row skipped","coords":{"report":"EURIZON 2023","page":"16","first_ref":"Leonardo Spa Az Nom","second_ref":"Leonardo","first":"row 12","second":"col 1"}}
```

JSON Lines rather than one JSON or YAML document, for two reasons that both come from the volume
this file sees at `-vvv` — tens of thousands of records for a single job. It **streams**: each
record reaches the buffered writer as it happens, so nothing accumulates in memory and the file is
readable even when the process dies, which is precisely the run whose log you most want to read.
And every line stands alone, so `grep` works on it and `jq` consumes it as a stream:

```console
$ jq -c 'select(.level == "WARN") | {activity, message}' freeports.log.jsonl
$ jq -r 'select(.error) | .error.display' freeports.log.jsonl | sort | uniq -c | sort -rn
```

When a run uses child processes, each child writes its own file in a private directory that is
deleted with it; the parent **absorbs** their contents into this one, in job order, before closing
it. There is one log per run, not one per process.

## `.log.csv`

The interesting one, and the only log that travels **with the output** rather than staying in the
working directory — because it is a product of the run and belongs with the run's other products.

A row appears only if the event carries a **page or a coordinate**. That single rule is what keeps
it an audit trail of the *extraction* rather than a transcript of the program: what was skipped, on
which page, at which position, and what was done about it.

| Column | From |
|---|---|
| `Report` | the `report` field, from the `document` span |
| `Page` | the `page` field |
| `Activity` | computed from the span stack |
| `First coord ref` | the **triggering text**: the report's own words that matched a company |
| `Second coord ref` | the second anchor — the company the triggering text matched, or the field a row is about |
| `First coord`, `Second coord` | the position itself, in units that depend on the context (`row 12`, `col 1`) |
| `Level` | the event's own severity — `WARN` or `ERROR` |
| `Message` | the event's message |

`Report` and `Activity` enrich a row but never justify one: an event carrying only those and no page
or coordinate produces no row. A document's name is not a position, and a `document` span is open
over a whole run — if it selected rows, every warning the program emits would become one.

`Level` is the same distinction the JSONL has carried all along, and the one this file used to leave
you to infer from the wording of the message. It separates the two severities the audit trail
deliberately keeps apart: `WARN` is the report being awkward and the engine coping — a dash where a
number belongs, a cast that had to be forced, a value sitting on the edge of its domain — while
`ERROR` is something that should not have happened, a value that is there and will not convert.
Only those two ever appear, because `warn` is this file's ceiling whatever `-v` you pass.

It is metadata rather than a field, which has one visible consequence: it is never inherited. A
warning raised inside an `info` span is a `WARN` row, not an `INFO` one. And since every event has a
level, a level alone never writes a row — the page-or-coordinate rule still decides that.

## `Report`

Which of the run's documents the row is about, so that a run over a batch is one file you can filter
rather than several you have to correlate. It comes from the `document` span, which is why an event
born deep inside a pipe carries it without knowing it exists. Empty for the rare row emitted outside
any document — a failure during configuration, before the first report is opened. The coordinate *refs* are deliberately not required to identify a point
uniquely — `Leonardo Spa Az Nom` is not a position, but it is something you can search for in a PDF
viewer, which is what someone checking a skipped row actually does.

That is also why `First coord ref` holds the **triggering text** rather than the company's name.
The two are not the same string: the triggering text is what the report wrote in the cell, and the
company is what the input database calls that issuer — `ITALY BTPS` for a row the report prints as
`Btps 1.3% 16-15.05.28 /Infl`. Only the first can be found again inside the document, and finding it
again is the whole purpose of the column. The company is not lost: it travels in `Second coord ref`
where the event has nothing more specific to say, it is the `Investee` column of `investments.csv`,
and for rows the deserializer produced it is also visible in `Activity`, whose `investment[…]`
segment carries the same anchor.

## `First coord` and `Second coord`

`row 12`, `col 1` — where in the page's table the row hooked itself. Every field of an investment
row is read at a fixed offset from that cell, so an anchor that landed on a header, a total or a
currency code shifts them all; the position is what shows it, as a column different from every other
row of the page.

Both count **from one**, because they are written to be read by a person matching a row against the
page, not fed back into the engine. The grid's own indices, the ones a format's offsets are computed
in, still count from zero.

The two travel further than they look. The table exists only inside the text filter — by the time
the deserializer sees the row it is a block, and the grid is gone — so the filter writes the position
into the block's metadata and the deserializer puts it back on the span wrapping the row. That is
what gives a coordinate to a failed cast or an out-of-range value, neither of which is emitted
anywhere near a table.

They are empty for anything that did not come from a table: a fund name, an SFDR article, a
management company. Empty is the honest answer there, not a missing feature.

**One row per event, not three per failure.** A lost field is a single row saying both what went
wrong and what was done about it — `"Error casting, skipping field: …"` — not an error row plus two
warnings about mitigation and consequence. The level already carries the severity and the message
the consequence. A *successful* mitigation does get its own row, because nothing was lost and that
is different information.

A run that dies before its configuration resolves writes no `.log.csv` at all, rather than leaving a
header-only file in the working directory: at that point it is not yet known where the output goes.
Nothing is lost — every one of those events also reached stderr and the JSONL file.

## What is *not* produced

Earlier versions wrote a fourth file, a YAML digest of the warnings and errors, at maximum
verbosity. It no longer exists at any verbosity: it duplicated in a second format records that
`freeports.log.jsonl` already carries in full, and only at the one verbosity where that file is at
its most complete.
