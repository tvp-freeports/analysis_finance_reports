# Formats

A **format** is the recipe for one report layout: `EURIZON-EN23`, `AMUNDI-IT24`. It carries the
publication year because a layout is a snapshot — next year's report from the same issuer is a
different format, not a revision of this one.

Formats do not live in the engine. They live in a **formats repository**, and this area is about
what you put in one and how you work on it. Why they live outside is {doc}`../design/formats-as-plugins`.

## The pages

| Page | Answers |
|---|---|
| {doc}`writing-a-format` | the whole path of adding support for a new report |
| {doc}`levels/structured` | rows in a spreadsheet, no code |
| {doc}`levels/semistructured` | a named algorithm plus YAML |
| {doc}`levels/unstructured` | a Python module, when a layout resists parameterisation |
| {doc}`selections` | how you say *where to look* on a page |
| {doc}`dev-loop` | inspect-document → inspect-page → make-tests → test |
| {doc}`repository` | the repository itself: layout, metadata, orchestration, versioning |
| {doc}`tooling` | `freeports-dev` and `freeports-validate`, option by option |

## Start here

```console
$ freeports-dev init-format-repo ~/work/my-formats
$ cd ~/work/my-formats
$ freeports-dev setup-input-db
```

Then {doc}`writing-a-format`.

```{toctree}
:maxdepth: 1
:hidden:

writing-a-format
levels/structured
levels/semistructured
levels/unstructured
selections
dev-loop
repository
tooling
```
