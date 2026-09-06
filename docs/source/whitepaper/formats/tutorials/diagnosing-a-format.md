# Diagnosing a format that already runs

The harder job is not the empty page. It is the format that runs, produces thousands of plausible
rows, and is quietly wrong about some of them. This page is a method for that, written from a real
investigation.

## Start from the log, not from the PDF

Every run writes `.log.csv` next to its output. It is a table, not a wall of text, and it exists so
that the first question — *where should I even look?* — can be answered by counting.

```console
$ python3 -c "
import csv, collections
rows = list(csv.DictReader(open('out/.log.csv')))
print(len(rows), 'rows')
for k, v in collections.Counter((r['Report'], r['Level']) for r in rows).most_common():
    print(v, k)"
188 rows
47 ('EURIZON-EN23-1', 'WARN')
30 ('AMUNDI-EN24', 'WARN')
12 ('CARNE-EN23', 'WARN')
```

Then count by *kind* of event rather than by document, because that is what tells you whether you
are looking at one problem or twenty. In the run above, 188 rows turned out to be four distinct
phenomena, of which two were not problems at all.

Each row carries the report, the page, the full activity path, and coordinates — the row and column
of the cell it is about. That is deliberate: the point of an event is that you can go back to the
report and *find the thing*.

## What the events mean

| The message says | It means | Usually |
|---|---|---|
| `… sits on the edge of the admissible range - kept` | a value landed exactly on a boundary of its domain — a holding worth zero, a position that is the whole fund | fine, and kept on purpose |
| `forced "…" to "…" to read it as a number` | the cell was not a clean number and noise had to be stripped before it could be read | worth a look |
| `… - holding skipped` / `page skipped` | something did not satisfy the output schema and was dropped, with the field named | a real loss, always read it |
| `the … is missing: N cells past the anchor` | the grid was walked and the expected cell was not there | a column mapping to re-check |

Two habits follow from that table.

**A warning is not automatically a bug.** The engine warns about zero-valued holdings because zero
is on the edge of what is admissible, not because zero is wrong — a security written off is worth
exactly nothing, and discarding it would throw away the very fact worth reporting. In the run above,
116 of the 188 rows were holdings rounding to 0.00% of net assets. All correct.

**And silence is not automatically correctness.** The events you get are the ones the engine knew
how to doubt. Nothing warns you that a column mapping is off by one and every market value belongs
to the row above.

## Two examples of the same message meaning opposite things

Both of these produced `forced "-X" to "X" to read it as a number`. One was noise, the other was a
real defect. Telling them apart is the whole skill.

### The false positive

A fund's Statement of Net Assets is laid out so that assets plus liabilities equal net assets, so
`Total Liabilities` is printed with a minus:

```text
Total Assets                                    215,581,172.39
Total Liabilities                                 -365,138.81
Net assets at the end of the financial year     215,216,033.58
```

The output schema wants a magnitude — it checks that `liabilities + net_assets == tot_assets` — so
the extracted value was right all along. The warning was an artefact of a convention, and the fix
was to make the convention explicit in the format rather than to leave the engine guessing.

The check that settled it took one command: compare the same field across every format that does
*not* warn, and see what value they carry. They were all positive magnitudes.

### The real defect

The same message, on a different report, was hiding this in the output:

```text
AMUNDI-EN24,317,EXXON MOBIL CORP - 110.00 - 16.08.24 PUT,Exxon Mobil,BOND,200.0,30900.0,USD,...
```

A **written put option** — a short position — recorded as a long holding of 30,900 USD in Exxon
Mobil, and classified as a bond. The minus sign that said "short" had been stripped.

Nothing about the message distinguished the two cases. What distinguished them was asking, in each
case, **what the sign meant in that field**: a presentation convention on a magnitude, or the
difference between owning something and owing it.

## Reproducing it on one page

Once you have a suspect page, the modes of `inspect-page` are a decision procedure rather than a
menu.

| Your question | Mode |
|---|---|
| how is this text actually typeset? | `structured` with `--strings` |
| did the engine read the columns the way I read them? | `table_ascii` or `table_md` |
| what was found on the page? | `pdf_blks` |
| what survived filtering? | `txt_blks` |
| what was it turned into? | `results` |

The table modes are the ones people forget, and they are the fastest route to a class of bug that is
otherwise invisible. Here is the page that held the Exxon put, rendered as a table:

```console
$ freeports-dev inspect-page -f AMUNDI-EN24 -p 249 -m table_ascii
     0                    1                      2         3      4                  5                     6        7
+---------+--------------------------------+-----------+-------+-----+--------------------------------+---------+-------+
| 81      | UBS AG LONDON CERTIFICATE      | 19,043    | 0.05  | -28 | NRG ENERGY INC - 75.00 -...    | -4,200  | -0.01 |  4
+---------+--------------------------------+-----------+-------+-----+--------------------------------+---------+-------+
```

Eight columns, because **the page holds two tables side by side**. That single observation explains
a whole family of confusing results, and it is worth internalising:

- reading order is **column-major** — down the left table, then down the right one;
- a section of the report can therefore open partway down one column and continue in the *other*
  column, or on the *next page*;
- so a rule of the form *"everything below y on this page"* is wrong twice over.

In the report above, the heading that opens the short-positions section sits in the right-hand column
of page 316, and the section runs on through the left-hand column of page 317. That is how a put
option ended up two pages away from the heading that explained it.

## Choosing the layer: is this the format's problem or the engine's?

The same symptom lives in different places, and putting the fix in the wrong one is how a repository
accumulates workarounds.

| Ask | If yes, it belongs to |
|---|---|
| would another report with this layout need the same handling? | **the format** |
| does the report state something the engine has no way to know? | **the format** |
| would every format be wrong in the same way on this input? | **the engine** |
| is the engine producing a value the report does not contain? | **the engine** |

The Exxon case answers the last one: the report said `-200`, the output said `200`. No format
configuration should have to compensate for that, so the fix went into the engine — and it went in
as *stop coercing*, not as *stop warning*.

```{important}
Never silence a diagnostic at the point where it is the only thing standing between a wrong value
and the output. If a coercion is what makes an invalid value pass validation, the fix is to remove
the coercion and let the field or the row be rejected with a message naming it — not to quieten the
message. A quiet log and a wrong number is the worst of the four combinations.
```

## Proving that a fix loses nothing

This is the part that is easy to skip and expensive to skip. A change that removes bad rows can
remove good ones, and *"I checked and it looks right"* does not scale to a thousand pages. Three
measurements, in increasing strength:

**1. The suite you already have.**

```console
$ freeports-dev test
259 passed
```

Every format's whole-document output is pinned, so anything you disturbed anywhere shows up. Do this
before you believe anything else.

**2. A real run, diffed.** The suite searches a small built-in company list, so a change that only
affects companies outside it passes invisibly. Run the engine against the input database you
actually care about, before and after, and diff the outputs.

**3. A run that emits everything.** Even a real database only surfaces rows that matched a company.
To see *every* row your change touches, build a throwaway input database with a single company whose
regex matches anything:

```text
Name,Bud,Regex
CATCHALL,,.
```

Every row of the report is now emitted, and the diff covers rows a realistic database would never
have reached. Expect noise — bare numbers get "matched" as company names — but this is the
measurement that turns *"nothing else seems to have changed"* into a count.

In the investigation this page is drawn from, those three steps produced: 259 tests passing, exactly
four rows removed from the real run and nothing altered, and, under the catch-all, confirmation that
every removed row was one the engine had previously emitted with a falsified sign.

## When the reference outputs must change

Sometimes a genuine improvement makes tests fail, because `out/**` is the repository's
specification and the specification was written against the old behaviour.

Regenerating is a deliberate act, and the rule is simple: **classify every single difference before
you touch anything.** Not "29 tests fail, let me regenerate" — but a list, of the shape:

- 13 coupon values, all corrected: `10.25%` was being read as `0.25%`;
- 14 per-page fixtures carrying those same coupons;
- 2 cells where `Intesa Sanpaolo SpA 7% / perpetual` moved from equity to **bond** — a perpetual has
  no maturity date, so it was only recognised once the coupon could be read;
- no rows lost, no row counts moved anywhere.

Once every difference has a reason, regenerate, and write the reasons down. If you cannot explain a
difference, you have found a second bug rather than finished with the first.

## The shape of the whole thing

```text
count the log        →  what kind of event, how many, where
name a hypothesis    →  in terms of a stage, not "the page is wrong"
reproduce on a page  →  inspect-page, the mode that answers your question
choose the layer     →  the format's problem or the engine's
prove no loss        →  the suite, a real run, a catch-all run
regenerate           →  only after every difference has a reason
```

The payoff, in the run this page follows: 188 log rows became 175, the twenty-one misleading
warnings became four accurate errors that name the security and the field, and coupon correctness
went from 2695 of 2758 to 2817 of 2817.
