# The input database

A run needs to know **which companies it is looking for** and **how to recognise each of them in a
report**. That is the input database: a directory, separate from the engine and from any formats
repository, pointed at with `--db-directory` / `-I`.

Keeping it separate is the same decision as keeping formats separate. Which companies matter is a
question about your work, not about PDF parsing, and two people using the same formats repository
will disagree about it — legitimately.

## The layout of a database
```text
companies/
  companies.csv                    Name,Bud,Regex
  companies_additional_buds.csv    Company name,Bud
  companies_additional_regexs.csv  Company name,Regex
  markets.csv                      Name
  tickers.csv                      Market name,Company name,Symbol
lists/
  lists.csv                        Name,Institution,Date
  company_to_list.csv              List name,Company name
```

## Companies, and how they are recognised

```text
Name,Bud,Regex
AP Møller Mærsk,maersk,\bmaersk
Airbnb,,\bairbnb\b
Alphabet,alphabet,\balphabet\b
```

Each company has a **name** — what ends up in the output, and the first thing looked for — plus up
to three kinds of evidence for finding it in text that was typeset for humans:

**Buds** are verbatim fragments that gate the cheap pass. The matcher tries a company's regexes
against a piece of text only if one of that company's buds occurs in it, which is what keeps a table
of hundreds of rows against hundreds of companies fast. A bud is therefore about *the text*, not
about the name.

**Regexes** are the patterns the name takes in practice — abbreviations, legal suffixes, group
names.

**Tickers** are exchange symbols, two to six upper-case letters, attached to a company and a market.

### The main table is checked against the name; the additional files are not

This is the distinction that matters, and it is not a matter of file shape.

The `Bud` and `Regex` columns of `companies.csv` describe **the company's own name**. So they are
checked against it: the bud must be already normalised and must actually occur in the normalised
name, and the regex must match that name. If you put an identifier here, it has to be consistent
with the name it sits next to — one that is not could only ever produce a false match or none, and
is rejected when the database is read.

The two `companies_additional_*` files are for **the other names the same company genuinely goes
by** — a former name, a brand, a parent group, a local subsidiary, the string a particular registrar
insists on printing. `Alphabet` is written `Google` in half the reports that hold it. These have no
reason to resemble the name in `companies.csv`, so **no such check is applied to them**: the only
validation is that the company they name exists in the main table.

Do not read that as laxity. It is the point of the two files: a bud or a regex that had to be
contained in the official name could not express the case they exist for.

One check does still reach an additional **bud**, and it is a different question from resembling the
name: it must be **already normalised**. A bud is compared verbatim against text the matcher has
normalised, so `ALADDIN` or `black-rock` would match nothing, ever, and say nothing about it — the
check is about the alphabet the comparison happens in. An additional **regex** is exempt, being a
pattern rather than a literal: anchors, character classes and escapes have no business being
normalised.

Matching runs on **normalised** forms — accents, case, punctuation and spacing removed, in three
increasing degrees — so that `Café Fund`, `CAFE  FUND` and `Cafe' Fund` are one name. The name is
kept exactly as written alongside the normalised form, and it is the written one that reaches the
output.

### The name is tried first, and only where a name can end

Before any bud or regex, the matcher asks the cheap question: does the text simply contain the
company's name? It is a substring search and not a pattern, and that is the point — it is what makes
the first pass survive hundreds of table rows against hundreds of companies.

Containment alone, though, attributes holdings to the wrong company, and the two cases that prove it
come from real reports:

| The text | The company | Why bare containment matches |
|---|---|---|
| `Other Assets` | `SSE` | `other a·sse·ts` |
| `Alphabeta Access Products Ltd.` | `Alphabet` | `alphabet·a access …` |

So the occurrence has to be **delimited**: no letter immediately before it, and none immediately
after. A digit is a perfectly good boundary, and so is punctuation — reports write `3M 2029`,
`SSE 4.75% 2031`, `ENI-SPA`, and a rule demanding a space would lose all three. Only a letter
touching the occurrence means the page is saying a longer word.

One subtlety is worth knowing, because it decides whether a holding is found. Normalisation
**erases** `.`, `/`, `'` and their kind rather than spacing them, so `AMAZON.COM INC` becomes
`amazoncom inc` and `Amazon` would appear to run into a letter the report never wrote next to it.
The text is therefore read **both ways** — as normalised, and with that punctuation restored as a
separator — and a delimited occurrence in either reading is a match. `AMAZON.COM INC`,
`BOOKING.COM` and `L'OREAL SA` are found; `Other Assets`, which has no punctuation to split on,
stays rejected.

This applies to the name only. Buds are gates for a regex that then decides, and the regexes
themselves say where their own boundaries are — `\balphabet\b` in the table above is doing exactly
that job by hand.

## Target lists, and their provenance
A run does not search for the whole database; it searches for **lists**, named with `--target-list`
/ `-T`. A list is a curated set of companies with a provenance:

```text
Name,Institution,Date
TEST,FREEPORTS,2025-01-01
```

```text
List name,Company name
TEST,AP Møller Mærsk
TEST,Airbnb
```

The institution and the date are not decoration. A list is normally somebody's published position —
an exclusion list, a sector definition, a screening — and the output of a run is only as
interpretable as the answer to "whose list, as of when". Recording it in the database means the
answer travels with the data.

Several lists can be given at once; their companies are unioned.

## Two properties that will surprise you if nobody says them

**File order is significant and is preserved, never sorted.** Matching is *first match wins*, so a
shorter name that is a prefix of a longer one must sit **after** the more specific one — exactly
where you put it. Sorting the file alphabetically would silently reattribute holdings to the wrong
company, which is the kind of error that produces a plausible number rather than an obvious failure.

**Patterns are matched unanchored.** They search the whole string. This is what the real matcher
does, and validation deliberately does the same, so a pattern that is accepted is a pattern that can
fire. `\bmaersk` against `ap moller maersk` is the case that decides it: anchoring the validation
would reject a pattern that works perfectly.

## Everything is validated on load

Company names unique; every bud and regex **of the main table** consistent with its own company's
name, as above; dates in `YYYY-MM-DD`; ticker symbols two to six upper-case letters; every
cross-reference — a list naming a company, a ticker naming a market, an additional bud or regex
naming a company — pointing at something that exists.

Additional buds and regexes are checked for that last cross-reference, plus — buds only — that they
are already normalised. A regex among them is not even compiled until the run needs it, so a syntax
error in one is reported when the matchers are built rather than when the file is read.

The database is read before any PDF is opened, so a mistake in it is reported in seconds rather than
after a long run produced a table with a company quietly missing from it.

## Getting an input database
`freeports-dev setup-input-db` writes a minimal database with a single list called `TEST`, which is
what a formats repository's own tests use. A real one is grown from it: the shape is small enough to
edit in a spreadsheet, and the validation is strict enough to catch what a spreadsheet gets wrong.

## Working on a database
There is nothing to build and no test suite of its own. A database is seven CSV files, and the loop
is the engine's own validation:

```text
edit the CSVs  →  point a run at the database  →  read the error  →  edit again
```

**Edit.** In a spreadsheet or a text editor, whichever suits the change. Two things a spreadsheet
will try to do to you: reordering rows, which changes matching because it is first-match-wins, and
"helpfully" rewriting a date or stripping a leading zero from a ticker. Check the diff before
committing, and keep the header row.

**Point a run at it** with `-I` / `--db-directory`:

```console
$ freeports -I ~/work/my-db -F ~/work/my-formats -i report.pdf -f EURIZON-EN23
```

The order in which a run fails is the useful part. The configuration is checked first — a path that
does not exist is reported in milliseconds, before anything is loaded — and the database is read
next, before any PDF is opened. So a mistake in the database costs you seconds, not the length of a
run, and it is reported as a specific row of a specific file rather than as a company quietly
missing from the output.

**Read the error, and believe it.** Almost everything the validator rejects is one of the checks
listed above: a bud in `companies.csv` that does not occur in its own company's name, a regex that
does not match the name it sits next to, a cross-reference to a company that is not in the main
table, a malformed date or ticker. The one class of error that arrives later is a regex with a
syntax error among the *additional* patterns: those are not compiled until the matchers are built.

```{note}
There is no validate-only command today — the cheapest check is a real run, which is quick because
the database is read first. If you are curating a large database and want a faster gate, that is a
sensible thing to ask for.
```

### Which database the tests use
A formats repository's tests use the repository's **own** `tests/input_db`, the minimal one
`setup-input-db` writes, and not a curated database. This is deliberate: a format's test must fail
because the *format* broke, never because somebody added a company to a list somewhere else. Do not
point a repository's tests at a real database to make them more realistic — you would be trading a
specification for a moving target.

### Changing a database somebody else maintains
Which companies matter is a question about the work being done, not about PDF parsing, and two
people using the same formats repository will legitimately disagree about it. A database is
therefore easy to fork and cheap to maintain: if your list differs, the answer is usually your own
database rather than a pull request against someone else's.
