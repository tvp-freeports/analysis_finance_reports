# Blocks

*Implemented.* `PdfBlock` and `TextBlock` are the two units that travel through a pipeline
({doc}`segments`). Both are the same three things:

| Part | Is |
|---|---|
| **block type** | a string in a newtype — an open set |
| **metadata** | free-form, a typed value enum |
| **content** | optional |

## The block type cannot be closed

It is a newtype over a string, not an enum, and it has to be: **formats repositories invent their own
block types**, and that is the extension mechanism working rather than a hole in the model. An enum
would mean every new kind of thing a report can contain requires a release of the engine.

The newtype still earns its keep. It gives a distinct type in signatures — so a block type cannot be
passed where a fund name was wanted — and a home for the standard constants that most formats use.

## The value is typed, the map is not

Metadata and content are a **typed value enum** rather than an untyped map of strings.

The consequence that matters is not internal: it is what lets serialisation be **derived** rather
than hand-written, and that in turn is what makes the per-page test fixtures ordinary **JSON** that
a person can read in a diff. A regression in a fixture should be something you can *see*
({doc}`../formats/dev-loop`).

## Comparing a block does not mutate it

That sounds too obvious to state. It is stated because the previous implementation hashed metadata
by **sorting it in place**, so hashing a block changed it, and `==` had side effects on its
operands.

It is the kind of bug that hides for years: everything works until two blocks are compared in a
different order than usual, and then a result changes for no visible reason. Recording it here is
cheaper than rediscovering it.
