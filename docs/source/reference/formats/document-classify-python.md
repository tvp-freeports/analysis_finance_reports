# The document classification module

When recognising a format takes more than the grammar of {doc}`document-classify-yaml` can say —
a computation over the lines of a page, a rule that depends on the page count — the author writes it
in Python. The module is optional, nothing creates it, and it runs **before** the YAML file.

## Where it lives

One of two layouts, as for every piece of author Python in a formats repository:

| Layout | When |
|---|---|
| `metadata/document_classify.py` | the logic fits in one file |
| `metadata/document_classify/__init__.py`, with any other files beside it | it does not |

Both present is an error, because which one is meant cannot be guessed:

```text
…/metadata/document_classify.py and …/metadata/document_classify/__init__.py both exist: keep one of the two layouts
```

Neither present is not an error: the repository simply has no custom classification. While the
module is imported, `content/templates/` is on the import path, so code shared with the formats can
be shared with the classifier too.

## What it must define

A variable `document_classifier`: a list of `(page, pipeline)` pairs.

```python
document_classifier = [
    (1, cover_pipeline),
    (2, contents_pipeline),
    (-1, back_cover_pipeline),
]
```

| Element | Is |
|---|---|
| the page | a non-zero integer — `1` the first page, `-1` the last |
| the pipeline | any object, or mapping, with `pdf_extract`, `text_filter` and `deserialize`, each a callable or an iterable of callables — the same contract, by shape, as the `pipelines` of a format module ({doc}`../../guides/formats/advanced/unstructured`) |

The entries are tried in the order written, and the search stops at the first one that settles a
complete name. A page is turned into lines once per document however many entries name it, and a page
the document does not have gives nothing for its entry. This is the same discipline as the YAML
file's pages, for the same reason: reading a page is the cost, so the cheapest test goes first.

An empty list is allowed and is the same as having no module.

## What a pipeline must produce

A pipeline's deserializers return a **string or `None`**. The `None`s are dropped and equal strings
merged; what remains must be nothing — no classification from this entry — or **one** string. Two
different strings from one pipeline are an error, not a choice between them.

The string is a format name, complete or partial ({doc}`names`), and it is checked when it is
produced against `metadata/formats.csv`, just as the YAML's names are checked at load time. A complete
name the repository does not declare, or a partial one that no format falls under, is an error.

What happens to a result follows the same rules as a YAML step:

| The pipeline returns | Given the result so far | Outcome |
|---|---|---|
| a complete name | that refines it | the document is that format; the search ends |
| a partial name | that strictly refines it | it becomes the result so far, and the search goes on |
| any name | that adds nothing to it | ignored, logged at debug level |
| any name | that contradicts it, or cannot be reconciled with it without losing part of either | ignored, with a **warning** naming the entry, the page, the name and the result so far |

The warning is there because two of the author's own pipelines disagreeing is a fault in the module,
and it should be seen without stopping the classification.

The one rule of the YAML file the module does **not** have is nesting: there are no blocks, so no
pipeline waits for another to have matched. A pipeline meant to fire only inside one entity repeats
that entity's test itself.

## Python first, then the YAML

| The module's search ends with | Then |
|---|---|
| a complete name | that is the result; the YAML file does not run |
| a partial name | the YAML file runs **starting from that partial name**, and can only refine it |
| nothing | the YAML file runs from nothing, as if there were no module |

The middle row is what makes the two worth combining. A module can do the one hard thing — say, tell
the entity from a computation no selection expresses — and leave the rest to the YAML blocks for that
entity, which are entered exactly as if a YAML step had found it.

## The YAML, redone in Python

Each top-level step of the YAML file is exactly a pipeline made of three standard pipes, which are
available from Python so that the declarative behaviour can be reproduced and extended:

| Segment | Pipe | Does |
|---|---|---|
| `pdf_extract` | `PdfExtractDocClassifyStandard(selections, pattern)` | gives `pattern` if **every** selection finds a line; with no selections, always gives it |
| `text_filter` | `TextFilterDocClassifyStandard()` | merges what the page gave, refusing two different names |
| `deserialize` | `DeserializerDocClassifyStandard()` | returns the name, or `None` |

`selections` is one `PdfLineSelection` or a list of them; `pattern` is a name, complete or partial,
and one that does not read as a name is a `ValueError` when the pipe is built.

These YAML steps

```yaml
- 1:
    KAIROS-EN23: {text: '^KAIROS INTERNATIONAL SICAV'}
    ASTERIA:     {text: '^Asteria Funds$', font: centurygothic-bold}
- 2:
    ASTERIA:
      EN24: {text: '^Asteria Funds$', font: centurygothic-bold}
      EN23: {text: '^Asteria Funds$', font: centurygothic}
```

are, in Python:

```python
from freeports.core import Pipeline
from freeports.standard_funcs.deserialize import DeserializerDocClassifyStandard
from freeports.standard_funcs.pdf_extract import PdfExtractDocClassifyStandard
from freeports.standard_funcs.text_filter import TextFilterDocClassifyStandard
from freeports.utils.pdf_extract import PdfLineSelection


def step(pattern, *selections):
    """One YAML step: the page gives `pattern` if every selection finds a line."""
    return Pipeline(
        pdf_extract=PdfExtractDocClassifyStandard(list(selections), pattern),
        text_filter=TextFilterDocClassifyStandard(),
        deserialize=DeserializerDocClassifyStandard(),
    )


document_classifier = [
    (1, step("KAIROS-EN23", PdfLineSelection(text="^KAIROS INTERNATIONAL SICAV"))),
    (1, step("ASTERIA", PdfLineSelection(text="^Asteria Funds$", font="centurygothic-bold"))),
    (2, step("ASTERIA-EN24", PdfLineSelection(text="^Asteria Funds$", font="centurygothic-bold"))),
    (2, step("ASTERIA-EN23", PdfLineSelection(text="^Asteria Funds$", font="centurygothic"))),
]
```

Three things the translation shows:

- **A sequence of steps is the same page repeated in the list.** Page 1 appears twice and is read
  once.
- **One pipeline per step, not one pipeline per page.** Putting both page-1 pipes in one pipeline
  would make a document that matches both an error, where the YAML would take the first.
- **The block became two plain entries, and lost its guard.** In the YAML, the page-2 block is
  entered only once `ASTERIA` has been found. The Python entries have no such condition: on a
  document where page 1 found nothing, a page-2 line `Asteria Funds` in Century Gothic Bold makes it
  `ASTERIA-EN24` on its own; where page 1 found another entity, the match contradicts it and is
  ignored with a warning. When the root's test is on the same page as the child's, repeating it in
  the child's selections restores the guard. When it is on another page, the simpler course is to
  stop the module at the partial name and leave the block to the YAML, which starts from it.

An unconditional step — the YAML's `~` — is `step("ASTERIA-EN23")`, with no selections. A
`PdfLineSelection` union, `a | b`, is the YAML's `any_of`.

Any of the three pipes can be replaced by the author's own callable, with the signatures of
{doc}`../../guides/formats/advanced/unstructured`, as long as the deserializer still returns a string
or `None`. That is the point of exposing them: the standard pipes are a starting point, not a
ceiling.

## What is refused

When the module is loaded:

| Message (after the module's path) | Caused by |
|---|---|
| `failed to import the document classification module: …` | the module raised while being imported |
| `the module defines no 'document_classifier'` | the variable is missing |
| `'document_classifier' must be a list of (page, pipeline) pairs, found a …` | it is not a list or tuple |
| `document_classifier[i] must be a (page, pipeline) pair, found …` | an entry is not a pair |
| `document_classifier[i]: the page must be an integer, found …` | a page that is not an `int` (a `bool` does not count) |
| `document_classifier[i]: page 0 does not exist (pages are 1-based, negative ones count from the end)` | page `0` |
| `document_classifier[i]: the value is not a pipeline (it has no '…')` | a pipeline missing one of the three segments |
| `document_classifier[i]: the pipeline has no pipe in …` | a segment left empty, so the pipeline could never produce anything |

When a document is classified, each error names the entry, its pipeline and its page:

| Message ends with | Caused by |
|---|---|
| `gave two different formats: '…' and '…'` | one pipeline producing two names |
| `returned …, not a document class` | a deserializer returning something other than a string or `None` |
| `gave '…', which is not a format of the repository` | a complete name `formats.csv` does not declare |
| `gave '…', which no format of the repository refines` | a partial name no format falls under |

Any of these stops the classification of that document, as does a page that cannot be read.
