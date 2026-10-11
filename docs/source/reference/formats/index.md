# Format names and document classification

A format is chosen by name, and the name is not decoration: it is built from what a document shows
on its first pages, in an order meant to put similar recipes next to each other. That is what makes
it possible both to *guess* a format for a document nobody has classified and to let a formats
repository *recognise* the format from the document itself.

This section is the reference for both. How to lay out a formats repository as a whole is
{doc}`../../guides/formats/repository`, where the two `metadata/document_classify` files sit beside
`formats.csv` and `url_mapping.csv`.

<!-- The toctree is hidden and sits here, above the first section, on purpose: placed at the
     end of the page it would be nested under whatever section came last, which is neither what
     the hierarchy is nor what the side panel should show. -->

```{toctree}
:maxdepth: 1
:hidden:

names
document-classify-yaml
document-classify-python
```

## What each page here answers

| Page | Answers |
|---|---|
| {doc}`names` | how a format is named, why in that order, and which format to try on a document nobody has classified |
| {doc}`document-classify-yaml` | how `metadata/document_classify.yaml` is written: pages, steps, nesting, partial names, and what is refused when it is loaded |
| {doc}`document-classify-python` | what `metadata/document_classify.py` must define, and how it hands over to the YAML |
