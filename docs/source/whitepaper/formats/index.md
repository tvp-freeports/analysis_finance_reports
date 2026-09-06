# Formats

A **format** is the recipe for one report layout: `EURIZON-EN23`, `AMUNDI-IT24`. It carries the
publication year because a layout is a snapshot — next year's report from the same issuer is a
different format, not a revision of this one.

Formats do not live in the engine. They live in a **formats repository**, and this area is about
what you put in one and how you work on it. Why they live outside is {doc}`../design/formats-as-plugins`.

<!-- The toctree is hidden and sits here, above the first section, on purpose: placed at the
     end of the page it would be nested under whatever section came last, which is neither what
     the hierarchy is nor what the side panel should show. -->

```{toctree}
:maxdepth: 1
:hidden:

tutorials/index
writing-a-format
levels/structured
levels/semistructured
levels/unstructured
selections
dev-loop
repository
tooling
configuration
```

## What each formats page answers
| Page | Answers |
|---|---|
| {doc}`tutorials/index` | walked-through examples: a first format, and diagnosing one that misbehaves |
| {doc}`writing-a-format` | the whole path of adding support for a new report |
| {doc}`levels/structured` | rows in a spreadsheet, no code |
| {doc}`levels/semistructured` | a named algorithm plus YAML |
| {doc}`levels/unstructured` | a Python module, when a layout resists parameterisation |
| {doc}`selections` | how you say *where to look* on a page |
| {doc}`dev-loop` | inspect-document → inspect-page → make-tests → test |
| {doc}`repository` | the repository itself: layout, metadata, orchestration, versioning |
| {doc}`tooling` | `freeports-dev` and `freeports-validate`, option by option |
| {doc}`configuration` | configuring those two the way the engine is configured |

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
