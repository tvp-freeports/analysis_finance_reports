# Your first format

We are going to add support for one report, end to end, and we are going to do it at the level where
you write no code at all. The report is a real one — the annual report of a Luxembourg SICAV, whose
format is called `ASTERIA-EN24` — and every console block on this page is real output, not an
illustration.

{doc}`../writing-a-format` is the same path as a checklist, once you have done it once.

## What we are aiming at

A format is a recipe for one report layout. When it works, this happens:

```console
$ freeports-dev test --format ASTERIA-EN24
6 passed
```

and the engine can read every report published in that layout.

## Step 0 — the two things you need

A formats repository and a report to work on.

```console
$ freeports-dev init-format-repo ~/work/my-formats
$ cd ~/work/my-formats
$ freeports-dev setup-input-db
```

`setup-input-db` writes a small list of well-known companies under `tests/input_db/`, called `TEST`.
It matters more than it looks: the engine only emits a holding whose investee is **in the list you
are searching**. A page full of holdings, none of them in your list, produces nothing at all — and
that is correct behaviour, not a failure. Remember this; it will confuse you at least once.

Then put the report where the tests will look for it:

```text
tests/formats/ASTERIA-EN24/report.pdf
```

## Step 1 — declare that the format exists

One row in `metadata/formats.csv`, written by components rather than by name:

```text
Name,Locale,Year,Country,Version
ASTERIA,EN,24,,
```

The name everything else refers to — `ASTERIA-EN24` — is assembled from those components, so a name
and its parts cannot drift apart. The year is not decoration: next year's report from the same
issuer is a *different format*, because a layout is a snapshot ({doc}`../repository`).

## Step 2 — ask the engine what it currently sees

```console
$ freeports-dev inspect-document --format ASTERIA-EN24
Page 1: unclassified
Page 2: unclassified
Page 3: unclassified
Page 4: unclassified
```

Everything is unclassified, and that is exactly right: we have not told it anything yet. This is the
starting line.

Now open the PDF and find a page of the table you actually want — the list of holdings. Ask
yourself one question: **what would let a machine recognise this page and not the others?** Usually
it is a heading. In this report every holdings page carries the line *"Transferable securities
admitted to an official stock exchange listing"*.

## Step 3 — find out how that heading is actually set

You could guess the font. Don't — there is a mode for this. Give `inspect-page` a string and it
tells you how the line is really typeset:

```console
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 33 -m structured \
    --strings "Transferable securities"
centurygothic-bold[7.982399940490723]((45.5306396484375:306.95733642578125)(157.61184692382812:167.39028930664062)) "Transferable securities admitted to an official stock exchange listing"
```

Read that as: font `centurygothic-bold`, size ≈ 7.98, occupying x 45.5→307.0 and y 157.6→167.4.

This one command is the answer to most *"what do I write here?"* questions. Whenever you are about
to invent a font name or a coordinate, search for the text instead and let the report tell you.

## Step 4 — classify the pages

Now write the rule, in `content/algorithms/structured/page_classify/args.csv`:

```text
ID,Header set,Class
ASTERIA-EN24,"CenturyGothic-Bold ""Transferable securities admitted to an official stock""",investments
```

That is a **selection** — a predicate over the lines of a page, here a font plus a piece of text.
The text is matched literally, with `^` and `$` available as anchors and nothing else; it is not a
regular expression ({doc}`../selections`). Note that we did not repeat the size: font plus that much
text is already unambiguous, and the fewer conditions you pin, the less likely a later page breaks
them.

Ask again:

```console
$ freeports-dev inspect-document --format ASTERIA-EN24
...
Page 33: investments
Page 34: investments
Page 35: investments
```

```{important}
**Do not go past this step until it is right.** Classification decides which pages the engine even
looks at. If a page you care about is not listed here, no amount of work on the extraction will make
it appear — and hunting for the reason in the wrong place is the single most common way to lose an
afternoon ({doc}`../../design/classification`).
```

## Step 5 — put the class on the schedule

A page class that no step mentions is an error, not a silent no-op. Add it to
`content/orchestration/algorithms_schedule.csv`:

```text
Format name,Page type,Filter next iteration
ASTERIA-EN24,investments,
```

One line, and for a single-class format that is all scheduling ever asks of you
({doc}`../../design/schedule`).

## Step 6 — read the table the way the engine reads it

Here is the part that makes the structured level feel easy. Ask for the page as a **table**:

```console
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 33 -m table_ascii
                0                    1       2        3           4        5
+-------------------------------+---------+-----+-----------+-----------+------+
| Cleanaway Waste Management... | 280,000 | AUD | 515,567   | 459,410   | 0.62 |  0
+-------------------------------+---------+-----+-----------+-----------+------+
| National Australia Bank Ltd.  | 3,719   | AUD | 92,589    | 85,427    | 0.12 |  1
+-------------------------------+---------+-----+-----------+-----------+------+
| REA Group Ltd.                | 615     | AUD | 96,536    | 88,839    | 0.12 |  2
+-------------------------------+---------+-----+-----------+-----------+------+
| Andritz AG                    | 11,067  | EUR | 630,336   | 561,305   | 0.76 |  3
+-------------------------------+---------+-----+-----------+-----------+------+
| Verbund AG                    | 2,996   | EUR | 254,063   | 217,165   | 0.29 |  4
+-------------------------------+---------+-----+-----------+-----------+------+
```

The engine has already worked out the grid. The numbers along the top are the column positions, and
naming the columns is now just reading them off:

| Column | Holds | Configuration field |
|---|---|---|
| 0 | the security's name | *(the anchor — not configured)* |
| 1 | how many | Quantity |
| 2 | the currency it was bought in | Acquisition currency |
| 3 | what it cost | Acquisition cost |
| 4 | what it is worth | Market value |
| 5 | share of net assets | % net assets |

So the extraction row, in `content/algorithms/structured/investments/args.csv`:

```text
ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency
ASTERIA-EN24,CenturyGothic-Bold(:87),"CenturyGothic-Bold[8.9802] ""(expressed in""",CenturyGothic (:810),4,1,5,3,2
```

The three selections before the numbers say where to find, in order, the sub-fund name, the sentence
that states the report's currency, and the body of the table. `(:87)` and `(:810)` are vertical
bands — *above y=87* for the sub-fund heading, *above y=810* for the body, which is how the page
footer is kept out. You find these the same way as everything else: search for a string, read the
coordinates it comes back with.

## Step 7 — walk the three stages

A page is processed in three stages, and you can print the output of each. Always in this order,
because the first one that looks wrong is the one to fix:

```console
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 36 -m pdf_blks   # what was found on the page
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 36 -m txt_blks   # what survived filtering
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 36 -m results    # what it was turned into
```

The middle stage keeps only holdings whose investee is in your list, and attaches everything it
understood:

```console
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 36 -m txt_blks
TextBlock(type_block="FUND", metadata={}, content="Asteria Funds - Planet Impact Global Equities")
TextBlock(type_block="EQUITY_TARGET", metadata={"% net assets": "1.11", "acquisition cost": "616,490", "acquisition currency": "TWD", "company": "TAIWAN SEMICONDUCTOR", "company match": "Taiwan Semiconductor Manufacturing Co. Ltd.", "currency": Currency.USD, "fund": "Asteria Funds - Planet Impact Global Equities", "manco": None, "market value": "816,173", "quantity": "24,891", "table col": 0, "table row": 2}, content="Taiwan Semiconductor Manufacturing Co. Ltd.")
```

Everything is still text at this stage — `"816,173"`, not a number. That is deliberate: reading the
layout and interpreting the meaning are different jobs, and keeping them apart is what lets you tell
a layout mistake from a units mistake ({doc}`../../design/segments`).

The last stage does the interpreting:

```console
$ freeports-dev inspect-page -f ASTERIA-EN24 -p 36 -m results
Equity({"company":"TAIWAN SEMICONDUCTOR","company_match":"Taiwan Semiconductor Manufacturing Co. Ltd.","fund":{"resolved":"Asteria Funds - Planet Impact Global Equities"},"nominal_quantity":24891.0,"market_value":{"resolved":816173.0},"currency":{"resolved":"USD"},"perc_net_assets":{"resolved":0.0111},"acquisition_cost":{"resolved":616490.0},"acquisition_currency":{"resolved":"TWD"}})
Fund({"name":{"resolved":"asteria funds planet impact global equities"}})
```

Numbers are numbers, the percentage is a fraction, and the fund name has been normalised.

```{note}
If `txt_blks` comes back **empty** on a page that clearly has holdings, the usual reason is not a
bug: none of the companies on that page is in the list you are searching. Try a page you know
contains one, or point at a bigger input database with `-I`.
```

## Step 8 — freeze what you have checked

```console
$ freeports-dev make-tests -f ASTERIA-EN24 -p 36
```

This writes three JSON fixtures for that page, one per stage. It shows you what it is about to write
and asks first — and you should genuinely read it, because `make-tests` records *what the code does
now*. Confirm a wrong number here and you have promoted a bug to a specification that the test suite
will defend from then on.

Freeze a handful of pages, chosen for being awkward rather than for being many: the first page of a
table, a page where it continues, a page with a row that wraps onto two lines. Those are where
formats break.

## Step 9 — run the whole report

```console
$ freeports-dev test --format ASTERIA-EN24
6 passed
```

Two kinds of test just ran. The per-page ones replay a single page through a single stage — fast,
and they name the stage that changed. The whole-document one replays the entire report and compares
every output table against `tests/formats/ASTERIA-EN24/out/`. That second one is what actually says
the format works: the per-page fixtures can all be green while the scheduling or the deduplication
across pages is wrong.

The first time, there is nothing to compare against, so generate it — deliberately, after reading
it. From that moment it is the format's specification, not a snapshot of a run.

## Step 10 — say who vouches for it

The reference output of a format is a claim about the world: *this company appears in this fund for
this amount.* The project records who is making the claim and on what basis, and that is what
`freeports-validate` is for ({doc}`../../validation`).

## When one row is not enough

Sooner or later a report will resist. The escape hatch is graded, not all-or-nothing: the three
levels **add up**, so you can keep the structured extraction and write only the filtering in Python,
or keep everything and override one deserializer. A hard format should cost more than a simple one —
but only in the one place that is hard ({doc}`../writing-a-format`).

Two things worth knowing before you meet them:

**Company matching is first-match-wins, in the input database's file order.** A holding attributed
to the wrong company is often a database problem, not an extraction one ({doc}`../../input-db`).

**A page that cannot answer everything by itself is not a broken page.** If the fund name is on the
cover and the table is on page 412, the table *promises* the name and the cover supplies it later.
Reach for that before you reach for making pages depend on each other
({doc}`../../design/promises`).
