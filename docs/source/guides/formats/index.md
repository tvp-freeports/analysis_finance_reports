# Formats

A **format** is the recipe for one report layout: `EURIZON-EN23`, `AMUNDI-IT24`. Its name carries
the issuer, the language and the **earliest** year of the reports it reads. Layouts drift over the
years, so a later report from the same issuer often needs a format of its own; when it does not, the
existing format reads it and keeps its name — names follow recipes, not documents
({doc}`../../reference/formats/names`).

Formats do not live in the engine. They live in a **formats repository**, and this area is about
what you put in one and how you work on it. Why they live outside is {doc}`../../reference/design/formats-as-plugins`.

<!-- The toctree is hidden and sits here, above the first section, on purpose: placed at the
     end of the page it would be nested under whatever section came last, which is neither what
     the hierarchy is nor what the side panel should show. -->

```{toctree}
:maxdepth: 1
:hidden:

tutorials/index
writing-a-format
adding-a-datum/index
dev-loop
selections
repository
advanced/index
```

## What each page here answers
| Page | Answers |
|---|---|
| {doc}`tutorials/index` | walked-through examples: a first format, and diagnosing one that misbehaves |
| {doc}`writing-a-format` | the whole path of adding support for a new report, as a checklist |
| {doc}`adding-a-datum/index` | the format exists and reads its holdings: now add the next datum to it |
| {doc}`dev-loop` | inspect-document → inspect-page → make-tests → test |
| {doc}`selections` | how you say *where to look* on a page |
| {doc}`repository` | the repository itself: layout, metadata, orchestration, versioning |
| {doc}`advanced/index` | the three levels a format can be written at, and how to choose |

## What is not here
Every option of `freeports-dev`, subcommand by subcommand, is
{doc}`../../reference/cli/freeports-dev`. Configuring it the way the engine is configured is
{doc}`../../reference/configuration/dev-and-validate`. Vouching for the reference output your tests
freeze is a different job with a different tool, and it is {doc}`../grants/index`.

## Starting a formats repository
```console
$ freeports-dev init-format-repo ~/work/my-formats
$ cd ~/work/my-formats
$ freeports-dev setup-input-db
```

If you have never added a format, go to {doc}`tutorials/first-format`: it does one real report from
an empty repository to a passing test suite, and a simple format turns out to be a single row in a
spreadsheet. {doc}`writing-a-format` is the same path as a checklist, for when you have done it
once.
