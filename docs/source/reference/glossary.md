# Glossary

Words this project uses in a particular way. Each entry says where the idea is treated properly.

```{glossary}
run
  One invocation of `freeports`. A run is one or more {term}`jobs <job>`, and everything they produce
  is accumulated and written **once**, at the end, as one set of tables. Running two reports in one
  run is therefore not the same as running them twice — see {doc}`../overview/how-it-works`.

job
  One report, read with one {term}`format`, against one set of target companies. Jobs come from
  repeated `--input` or from the rows of a batch CSV ({doc}`../guides/user/advanced/batch`).

format
  The recipe for one report layout, named for the issuer and the publication year —
  `EURIZON-EN23`. A layout is a snapshot, so next year's report from the same issuer is a different
  format rather than a revision. {doc}`../guides/formats/index`.

formats repository
  A repository holding formats, their tests and their validation documents, maintained
  independently of the engine. Created with `freeports-dev init-format-repo`.
  {doc}`../guides/formats/repository`.

input database
  A directory of CSV files saying **which companies a run looks for** and how to recognise each one.
  Separate from the engine and from any formats repository, because which companies matter is a
  question about your work rather than about parsing PDFs. {doc}`../guides/input-db/index`.

bud
  A fragment of a company name used to recognise it in a report — the part that survives the
  variations issuers introduce. {doc}`../guides/input-db/recognising-companies`.

target list
  A named set of companies within an input database. A run needs at least one, and refuses
  immediately if none is given. {doc}`../guides/input-db/target-lists`.

page class
  What kind of page this is, decided per document before any extraction happens. Classification is
  itself a pipeline. {doc}`design/classification`.

schedule
  The ordered list of work for a run: the classified pages of every document, poured together and
  processed in steps, where each step's results filter the next. {doc}`design/schedule`.

pipeline
  What a page of a given class goes through: three {term}`segments <segment>` —
  `pdf_extract` → `text_filter` → `deserialize`. A class runs a *bundle* of them.
  {doc}`design/segments`.

segment
  One of the three stages of a pipeline, answering one separable question: what is on this page,
  does any of it concern us, what do the survivors mean. {doc}`design/segments`.

promise
  A placeholder for a value a page genuinely could not know, resolved at the end of the run against
  everything the run learned. {doc}`design/promises`.

entity
  What the project models and ultimately writes out — the thing a resolved extraction becomes.
  {doc}`design/entities-and-output`.

profile
  A choice of what shape the output takes: which tables are written and where.
  {doc}`../guides/user/output`.

grant
  A signed statement that a named {term}`methodology` was applied to a specific file, recorded
  against that file's hash — and therefore invalidated by any change to the file, deliberately.
  {doc}`../guides/grants/index`.

methodology
  A published document describing a protocol under which a grant may be made. It is a document at
  an address the granter chose to trust, not a value stored in the grant.
  {doc}`cli/methodology-sources`.

validation document
  One YAML file per contributor, at `<repo>/validation/<name>.yaml`, signed with their key, holding
  every grant that person has made in that repository.
  {doc}`cli/freeports-validate`.

assertion
  A statement about this software, written as prose, that a person can read and agree with. Grants
  over assertions are made under *agreement and good faith* rather than over bytes a program wrote.
  {doc}`../validation/index`.

metric
  A dotted name, a unit and a cost class, with a minimum in `ci.yaml`. Distinct from a **suite**,
  which has an outcome rather than a number. {doc}`../guides/devops/ci-yaml`.

branch class
  Which of `dev`, `pre` or `prod` a branch belongs to, derived from its name at every run and never
  recorded. It decides what the gate refuses. {doc}`../guides/devops/the-gate`.
```
