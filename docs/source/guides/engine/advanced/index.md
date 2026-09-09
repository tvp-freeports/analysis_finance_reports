# The engine, deeper

One page, and it is the one that answers "why is it built out of *these* pieces".

```{toctree}
:maxdepth: 1
:hidden:

implementation-notes
```

{doc}`implementation-notes` covers the **technology** choices — which crates, which bindings, what
was tried and rejected. That is a different question from why the *algorithm* has the shape it has,
which is {doc}`../../../reference/design/index`. Keeping the two apart is deliberate: a library can
be replaced without the algorithm changing, and the algorithm can change without any library moving.
