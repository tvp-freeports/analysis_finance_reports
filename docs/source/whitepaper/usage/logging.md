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
 WARN run/job[EURIZON 2023]/step[0]/class[investments]/page[16]/pipeline[investments]/text_filter/pipe[TextFilterInvestmentsStandard]:
      expected text block not found near the matched company - row skipped coord_ref_1=Leonardo
```

That path is the point. It says *what was happening* — job, step, page class, page, pipeline,
segment, pipe — rather than which source file the line came from, which is what you want when a
format misbehaves on one page of one report.

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
{"time":"2026-08-31T09:14:02.118773Z","level":"WARN","activity":"run/job[EURIZON 2023]/step[0]/class[investments]/page[16]/pipeline[investments]/text_filter/pipe[TextFilterInvestmentsStandard]","target":"freeports::formats_utils::text_filter","message":"expected text block not found near the matched company - row skipped","coords":{"page":"16","first_ref":"Leonardo"}}
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
| `Page` | the `page` field |
| `Activity` | computed from the span stack |
| `First coord ref`, `Second coord ref` | textual anchors to a position — a matched company, a field name |
| `First coord`, `Second coord` | the position itself, in units that depend on the context (`row 12`, `col 3`) |
| `Message` | the event's message |

`Activity` enriches a row but never justifies one: an event carrying only an activity and no page or
coordinate produces no row. The coordinate *refs* are deliberately not required to identify a point
uniquely — `Leonardo` is not a position, but it is something you can search for in a PDF viewer,
which is what someone checking a skipped row actually does.

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
