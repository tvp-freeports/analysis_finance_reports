# The unstructured level

When a layout resists parameterisation, write it. One format, one Python module under
`content/algorithms/unstructured/`.

```python
from freeports.core import Pipeline
from freeports.standard_funcs.pdf_extract import PdfExtractInvestmentsStandard
from freeports.utils.pdf_extract import PdfLineSelection

body_set = PdfLineSelection(font="ArialMT", font_size=(6.95, 6.97))

pipelines = {
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(body_set=body_set),
    ),
}
```

The module exports a `pipelines` mapping. Two things about that example matter more than the code
itself.

## The contract is by shape, not by type

`pipelines` maps a name to **any object** with `pdf_extract`, `text_filter` and `deserialize`
attributes, each a callable or an iterable of callables.

The engine's own `Pipeline` satisfies that protocol, but a plain object — or a dictionary — works
just as well. Which is what lets a format be written **without importing the engine at all**, and
means a repository's Python is testable on its own terms.

## Native and author pipes mix inside one segment

The example does exactly that: a **library** pipe configured with a selection **you** built.

A native pipe passed in from Python is unwrapped back to its Rust form rather than being called
through Python for every block. Only your own callables stay wrapped, and they are the only thing
that pays the boundary cost.

That has a visible consequence for speed: an author's pipe declares that it does **not** scale with
threads, so a page class handled by one runs sequentially rather than contending for the GIL
({doc}`../../design/parallelism`). A format that gains nothing from `--pages` is usually one of
these, and that is the design working.

## The classification finalizer

A module may also export `compute_page_class`:

```python
def compute_page_class(classes):
    """classes: the raw per-page classification of ONE document."""
    ...
    return resolved
```

It receives the raw per-page classes of **one document** and returns the resolved ones. This is where
*"this page continues the table that started on the previous one"* is expressed, and it is the only
place in the engine where reasoning across pages is available at all
({doc}`../../design/classification`).

## Errors

A Python exception raised by your pipe is logged **with its traceback** and converted to a typed
error at the boundary. No Python exception travels further into the engine, so a format cannot crash
a run in a way that loses the other formats' results — the job fails, and it fails with a message
naming the format, the page and the pipe.
