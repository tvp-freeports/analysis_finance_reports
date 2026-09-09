# Reference, by topic

This is the **topic** axis of the documentation: audience-independent, complete, and the place every
guide points at rather than repeating. A page here answers *what does this do*, for every option and
every setting, without caring who is asking or what they are trying to achieve.

If you want to know which of these you need, you are asking a guide's question:
{doc}`../guides/index`.

```{toctree}
:maxdepth: 2
:hidden:

cli/index
configuration/index
design/index
../validation/index
glossary
misc
```

## What each part covers
| Part | Covers |
|---|---|
| {doc}`cli/index` | every option of `freeports`, `freeports-dev` and `freeports-validate` |
| {doc}`configuration/index` | the four places a setting can come from, and how they merge |
| {doc}`design/index` | the algorithm, chapter by chapter, with each claim marked implemented, planned or accepted limit |
| {doc}`../validation/index` | the published methodologies themselves — hashed artefacts, not ordinary prose |
| {doc}`glossary` | the words this project uses in a particular way |
| {doc}`misc` | whatever a section has not been written for yet |

## Two things to know about this column
**A reference page does not advise.** It says what an option does and stops. "Which of these do I
want" is answered in the guide you came from — deliberately, because the answer differs by reader
and the option does not.

**The validation pages are not editable prose.** They are content-addressed: their hashes are
recorded in signed grants, here and in other repositories, and their published URLs are what those
grants resolve. Correcting a typo in one invalidates every grant that cites it. See
{doc}`../guides/institutional/why-trust-a-grant`.
