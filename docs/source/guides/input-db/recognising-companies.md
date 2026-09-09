# Companies, and how they are recognised

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

## The main table is checked against the name; the additional files are not

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

## The name is tried first, and only where a name can end

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
