# The semistructured level

Between the two others. The algorithm has a **name**; you say which name serves which segment, and
configure it in YAML.

## The two files

`content/algorithms/semistructured/formats_mapping.csv` says which named algorithm serves which
segment of which pipeline:

```text
ID,pdf_extract,text_filter,deserialize
AMUNDI-IT24(investments),standard_cost_curr,,
```

An empty cell leaves that segment unspecified at this level, so another level can supply it.

One YAML file per segment carries the configuration, under `args/`:

```yaml
AMUNDI-IT24(investments):
  body_set:
    font: TrebuchetMS
  subfund_set:
    font: Arial-BoldItalicMT
    area:
      y_max: 60
  currency: EUR
  tolerance: 1.0
```

## The key

`{format}({pipeline})`, falling back to the bare `{format}` only for the **unnamed** pipeline. The
two forms are not interchangeable: writing `AMUNDI-IT24` when the pipeline has a name will not be
found, and the failure is silent in the sense that the segment simply has no configuration.

## Lists, and the counter that indexes them

Where a value is a **list**, the element used is chosen by **how many pipes have already been emitted
for that pipeline and segment** — not by the row's position in the mapping table.

```yaml
CARNE-EN23(investments):
  body_set:
    - { font: ArialMT }        # first pipe of this pipeline+segment
    - { font: Arial-BoldMT }   # second
```

An algorithm that returns three pipes advances that counter by **three**. This is the rule that
surprises people, and it is worth restating: the counter follows the pipes actually produced, so
adding an algorithm earlier in the same segment shifts every list index after it.

## Your own names

An algorithm name may be implemented natively or by you, in your repository's Python.

Defining the same name **both** ways is an **error**, not a precedence to be remembered. The check
runs over every native name of the segment as soon as your module loads.

A collision is nearly always a typo, and letting one side win silently is what makes a typo
expensive — you would be debugging an algorithm you did not write while reading the one you did.
