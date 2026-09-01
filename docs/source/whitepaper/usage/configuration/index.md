# Configuration

Nothing about a run has to be said on the command line. A setting can come from four places, and
this area describes each of them plus the options they all share.

| Page | Answers |
|---|---|
| {doc}`options` | the canonical options: type, default, validation, and how each source spells it |
| {doc}`cmd_args` | command line → option |
| {doc}`env_variables` | environment variable → option |
| {doc}`config_file` | YAML key → option, and where the file is looked for |
| {doc}`batch_rows` | batch CSV column → option |

The same four sources, the same precedence and the same per-field merge also configure
`freeports-dev` and `freeports-validate`, through two optional sections of the configuration file
and two environment prefixes of their own. Both commands are only run by format authors, so that is
documented where they are: {doc}`../../formats/configuration`.

<!-- The toctree is hidden and sits here, above the first section, on purpose: placed at the
     end of the page it would be nested under whatever section came last, which is neither what
     the hierarchy is nor what the side panel should show. -->

```{toctree}
:maxdepth: 1
:hidden:

options
cmd_args
env_variables
config_file
batch_rows
```

## Four sources, one order

Precedence, from weakest to strongest:

```text
   defaults  ◄──  configuration file  ◄──  environment  ◄──  command line  ◄──  batch row
   weakest                                                                      strongest
```

The **batch row wins over the command line** on purpose. In batch mode the command line describes
the *run* and the row describes *this job*, and the more specific statement should be the one that
holds.

## Merged per field, not per source

Each source is read into a *partial* configuration — partial because no source is obliged to say
everything — and the merge is **field by field**. A configuration file may set the output path and
the environment the formats repository, and neither erases the other.

**Per field means per field, not per group.** Every setting that looks like one thing made of two —
the two parallelism levels, the two output flags — travels the merge as two independent values and
is only reassembled at the end. So a file that turns the archive on and an environment that turns
separate output on produce a run with **both**, not a run with whichever source came last:

```yaml
# freeports-config.yaml
out_flags:
  archive: true
```

```console
$ FREEPORTS_SEPARATE_OUT=true freeports …    # → archive AND separate out
```

The alternative would be an override nobody can see, which is the failure this whole design refuses.

## Absent is not false

A source that says nothing about a field leaves it **unset**, and a lower tier still decides it.
This is why the command line can only switch a boolean flag *on*: `--archive` present means true,
`--archive` absent means *nothing said* — otherwise every command line would silently switch off
what a configuration file had turned on.

To turn a flag back off, say so where `false` can be written: `FREEPORTS_ARCHIVE=false`, or
`archive: false` in the file.

## Validation happens once, in a fixed order

Only the merged result is validated, and the order is part of the behaviour:

1. **require the target lists** — a pure presence check, so a run missing them fails immediately
   rather than after four minutes of PDF;
2. **detect the format**, where none was given;
3. **validate the document specifiers** ({doc}`../documents`);
4. **set the archive flag**, which must come before the next step because it can change the output
   path;
5. **check that the output path's parent exists**;
6. **check the single-file profile's path**, appending `out.csv` where needed.

## Unknown means error, everywhere

An unknown YAML key, an unknown sub-key of `parallelism` or `out_flags`, an unknown batch column:
all are explicit errors. A misspelled setting that is quietly ignored configures nothing and reports
nothing, and the user is left believing it took effect.

The rule also refuses settings that were *considered and never implemented*. `out_flags: compressed:`
is rejected even though `compressed` is the field's internal name, because accepting it would make an
option that does not exist look active.

## The one setting that does not work: verbosity
```{warning}
`FREEPORTS_VERBOSITY` and the configuration file's `verbosity` key are **parsed, validated and
merged, but not applied** to the logging of an ordinary run. The parent process installs its logging
from the `-v` and `-q` counts alone, before the configuration is resolved, and never revisits it.

Measured, not inferred: with `FREEPORTS_VERBOSITY=trace` and no flags, stderr stays at warning level.

Worker child processes *do* honour the resolved value, since it reaches them inside their request —
so a parent and its children can disagree about verbosity in the same run. Today the verbosity of the
process you are watching can only be set with `-v` and `-q`.
```
