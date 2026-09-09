# Running the engine

You have `freeports` installed and you want tables out of a report. This section is the loop: what
must exist before a run can work, how to name a document, what comes out, and how to read the logs
when it does not do what you expected.

```{toctree}
:maxdepth: 1
:hidden:

inputs
documents
output
logging
advanced/index
```

## What each page answers
| Page | Answers |
|---|---|
| {doc}`inputs` | what must exist before a first run can work at all |
| {doc}`documents` | how to name a document: the `<url>:<path>:<name>` grammar, and `save_pdf` |
| {doc}`output` | the profiles, the tables produced, the rules that hold across all of them |
| {doc}`logging` | the three destinations, where each lands, how to read them |
| {doc}`advanced/index` | one job per CSV row, and driving the two levels of parallelism |

If you have not run it once yet, {doc}`../../start/first-run` is a single real invocation commented
argument by argument, and {doc}`../../start/install` is what to install first.

## What is not on this axis
Every **option** of the command, with its type, default and validation, is
{doc}`../../reference/cli/freeports` — a reference page, because the answer does not depend on who
is asking. The four places a setting can come from and how they merge is
{doc}`../../reference/configuration/index`, which is what you want as soon as your command lines
get long.

*Why* a run behaves the way it does — why two reports in one invocation is not the same as two
invocations, why the order of the output is guaranteed in some respects and not others — is
{doc}`../../overview/how-it-works` briefly and {doc}`../../reference/design/index` properly.
