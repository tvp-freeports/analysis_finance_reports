# Beyond a single run

Two things you do not need for a first run and will want for a hundredth: driving many jobs from a
file, and deciding how much of the machine to use.

```{toctree}
:maxdepth: 1
:hidden:

batch
parallelism
```

| Page | Answers |
|---|---|
| {doc}`batch` | one job per CSV row, and why a row wins over the command line |
| {doc}`parallelism` | the two levels, `auto`, the measurements, and the price in memory |

Both are configured like everything else — {doc}`../../../reference/configuration/index` — and both
are consequences of the design rather than features bolted on: {doc}`../../../reference/design/parallelism`
explains why parallelism falls out of the page assumption instead of being added to it.
