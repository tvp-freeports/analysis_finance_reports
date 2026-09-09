# Why this project exists

## The obligation is on the publication, not on the shape
Funds and the companies managing them must disclose what they hold, what it is worth, who runs it
and how it is classified under sustainability regulation. They do. The disclosure is an annual
report, published as a PDF, laid out however that issuer's typesetter chose.

Nothing requires two issuers to agree on the shape of that document, and they do not. The same fact
— this fund holds this much of this company — is printed in several hundred mutually incompatible
ways, in documents built for a person to read one at a time.

## Which leaves three options, and two of them are bad
**Read them by hand.** Correct, and it does not scale past a handful of reports. It is also not
repeatable: the next person to ask the same question starts again.

**Buy the data from a vendor.** Someone else read them by hand, months ago, and now sells the
result. You inherit their errors without being able to see them, their coverage without being able
to extend it, and their timing.

**Write a program.** Which works, once, for one issuer, until next year's report moves a column.
Done naively this is the worst of the three: a hundred fragile programs nobody maintains.

## The third option, made maintainable
This project is the third option with the fragility factored out. There is an **engine** that knows
nothing about any particular report, and there are **formats** — small recipes, one per report
layout — that tell it where to look. The engine is the part that must not change. The formats are
the part that grows, and they live in separate repositories maintained by whoever cares about those
issuers.

The same split happens again for *which companies matter*: that lives in an **input database**,
also separate, because it is a question about somebody's work rather than about parsing PDFs. Two
people using the same formats can legitimately disagree about it.

{doc}`../../overview/how-it-works` is what the engine does with all that;
{doc}`../../overview/the-repositories` is the map of who maintains which piece.

## What it is used for
The immediate motivation is being able to ask a question like *which funds hold companies on this
list, and how much* across many issuers at once, and to be able to show your working when somebody
disputes the answer. That "show your working" is not decoration — it is the reason the project
records who vouched for what, under which published methodology, against which exact bytes.

Whether that is enough for your purpose is a real question, and {doc}`strengths-and-limits` is the
honest answer rather than the encouraging one.
