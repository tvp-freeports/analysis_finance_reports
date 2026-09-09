# The input database

A run needs to know **which companies it is looking for** and **how to recognise each of them in a
report**. That is the input database: a directory, separate from the engine and from any formats
repository, pointed at with `--db-directory` / `-I`.

Keeping it separate is the same decision as keeping formats separate. Which companies matter is a
question about your work, not about PDF parsing, and two people using the same formats repository
will disagree about it — legitimately.

```{toctree}
:maxdepth: 1
:hidden:

recognising-companies
target-lists
working-on-a-database
```

## What each page here answers
| Page | Answers |
|---|---|
| {doc}`recognising-companies` | how a company is matched in a report: buds, regexes, and which file is checked against what |
| {doc}`target-lists` | which companies a run actually looks for, and where a list came from |
| {doc}`working-on-a-database` | getting one, changing one, and what is validated on load |

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

## What is not here
`--db-directory` and `--target-list` as options, with their defaults and validation, are
{doc}`../../reference/cli/freeports`. There is no build and no test suite in an input database: the
loop is the engine's own validation, which runs **before any PDF is opened** and therefore answers
in seconds.
