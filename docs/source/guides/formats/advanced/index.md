# The three levels of a format

Most formats never leave the first level. These pages are the other two, and the rule for choosing
between them: **go up a level only when the level below genuinely cannot express the layout.**

```{toctree}
:maxdepth: 1
:hidden:

structured
semistructured
unstructured
```

| Level | Is | Reach for it when |
|---|---|---|
| {doc}`structured` | rows in a spreadsheet, no code | the layout is a table the engine can already read |
| {doc}`semistructured` | a named algorithm plus YAML | the shape is regular but needs parameters |
| {doc}`unstructured` | a Python module | the layout resists parameterisation altogether |

How the three merge, and why they are outside the engine at all, is
{doc}`../../../reference/design/formats-as-plugins`.
