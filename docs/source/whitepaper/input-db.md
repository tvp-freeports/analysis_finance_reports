# The input database

A run needs to know **which companies it is looking for** and **how to recognise each of them in a
report**. That is the input database: a directory, separate from the engine and from any formats
repository, pointed at with `--db-directory` / `-I`.

Keeping it separate is the same decision as keeping formats separate. Which companies matter is a
question about your work, not about PDF parsing, and two people using the same formats repository
will disagree about it — legitimately.

## Layout

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

Each company has a **name** — what ends up in the output — and up to three kinds of evidence for
finding it in text that was typeset for humans:

**Buds** are verbatim fragments that must be present. They are given already normalised, and each
must actually occur in the company's own normalised name; a bud that does not is rejected when the
database is read, because it could only ever produce a false match or none.

**Regexes** are the patterns the name takes in practice — abbreviations, legal suffixes, group
names. Each must match its own company's name, for the same reason.

**Tickers** are exchange symbols, two to six upper-case letters, attached to a company and a market.

The two `companies_additional_*` files exist so that a company can have more than one bud or regex
without the main table growing columns. The distinction is only about file shape; the matcher treats
them the same.

Matching runs on **normalised** forms — accents, case, punctuation and spacing removed, in three
increasing degrees — so that `Café Fund`, `CAFE  FUND` and `Cafe' Fund` are one name. The name is
kept exactly as written alongside the normalised form, and it is the written one that reaches the
output.

## Lists

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

Company names unique; every bud already normalised and contained in its company's normalised name;
every regex matching that name; dates in `YYYY-MM-DD`; ticker symbols two to six upper-case letters;
every cross-reference — a list naming a company, a ticker naming a market — pointing at something
that exists.

The database is read before any PDF is opened, so a mistake in it is reported in seconds rather than
after a long run produced a table with a company quietly missing from it.

## Getting one

`freeports-dev setup-input-db` writes a minimal database with a single list called `TEST`, which is
what a formats repository's own tests use. A real one is grown from it: the shape is small enough to
edit in a spreadsheet, and the validation is strict enough to catch what a spreadsheet gets wrong.
