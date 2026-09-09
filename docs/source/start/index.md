# Quick start

Everything in this section is read by everyone, whatever you came here to do: what to install, what
one real run looks like, and — if you intend to change something rather than only use it — which of
the eight kinds of contribution is yours.

```{toctree}
:maxdepth: 1
:hidden:

install
first-run
dev-tools
ways-to-contribute
```

## The four pages here
| Page | Answers |
|---|---|
| {doc}`install` | what to install, and which of the three distributions you need |
| {doc}`first-run` | one real run, commented argument by argument |
| {doc}`dev-tools` | the developer toolchain, and what each `make` target sets up |
| {doc}`ways-to-contribute` | the eight shapes a contribution takes, and where each one is documented |

## The four things a run needs
Four things must be given, and only one of them can ever be worked out by the engine on its own:

| What | How | Can be inferred? |
|---|---|---|
| a document | `--input` / `-i` | no |
| a format for it | `--format` / `-f` | **yes**, from a URL the formats repository recognises |
| a formats repository | `--repo` / `-F` | no |
| an input database, and at least one list in it | `--db-directory` / `-I`, `--target-list` / `-T` | no |

```console
$ freeports --input report.pdf --format EURIZON-EN23 \
            --repo /path/to/formats-repo --db-directory /path/to/input-db \
            --target-list controversial_weapons --out ./results
```

None of the four has a useful default, and the engine refuses rather than guessing. If no target
list is given the run stops immediately, before opening anything — the check is first because
failing fast on a missing input is kinder than failing after four minutes of PDF.

Two of those four are separate repositories, maintained by other people:
{doc}`../guides/formats/index` is where a format comes from, {doc}`../guides/input-db/index` is
where the company lists come from. {doc}`../overview/the-repositories` is the whole map.

## Where to go next
{doc}`../overview/index` if you want to understand the thing before driving it;
{doc}`../guides/user/index` if you want to drive it now; {doc}`ways-to-contribute` if you intend to
change it.
