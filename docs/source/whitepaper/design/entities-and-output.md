# Entities, and the output schema

*Implemented.* What the project models, and why the shape of the output is treated as an invariant
rather than as a formatting choice.

## What comes out of a pipeline

`Extracted` is the union of the entity kinds the project models — equity, bond, fund, fund assets,
SFDR classification, ESG indicator, rename, merge, management company, investments manager — **plus
one member that is not an entity at all**: the promise entries a pipe deposited ({doc}`promises`).

Putting promises in the same union is what lets a pipe say *"this page produced a value for someone
else"* through the ordinary return path, rather than through a side channel that every segment would
have to know about.

## The entity list is a modelling decision

Ten kinds, not a generic bag of key-value pairs. Each is a thing a reader of financial reports asks
about, and each has fields with types and domains — a currency is a currency, a date is a date, a
percentage is bounded.

**Rejected: a generic row type** with untyped columns, which would have absorbed any format without
change. It would also have pushed every question of meaning to the consumer: whether two rows
describe the same fund, whether a number is a percentage or a fraction, whether an empty cell is
zero or unknown. The value of this project is precisely in having answered those, so a generic
model would have given away the product to save work on the engine.

## The schema is an invariant of the product

The row structures carry their **own** validation: the domain each numeric field must fall in, and
the key on which two rows count as the same. That validation lives with the schema rather than with
the code that writes files.

The reason is a claim about *what kind of fact* it is. An output file that violates one of these is
**wrong even if every step that produced it was right** — it is not a bug in the writer, it is a
statement about the world that cannot be true. Validation attached to the writing would be checking
the writer; validation attached to the schema is checking the claim.

## The three rules a consumer can rely on

They hold across every profile, and they exist so nobody has to guess
({doc}`../usage/output`):

**Every CSV always has its header, even with no rows.** A table with nothing in it is a header-only
file, never a missing one — so *"nothing found"* and *"no such table"* stay distinguishable. This is
the rule that costs the most to keep and is worth the most: the alternative makes every consumer
write a special case for a file that may not exist, and get it wrong.

**Headers are exact**, in text and in order. A consumer may index by position.

**An absent optional value is an empty cell** — never `None`, never `null`, never `NaN` — and a
floating-point number always carries at least one decimal. One representation of absence, and
numbers that never change type between rows.

## One set of tables per run

Resolved entities accumulate across every page, every document and every job, and are written
**once**, at the end. Not per document, not per job, and — critically — not per child process when
jobs run in parallel: the children send results back and the parent writes.

That is why job-level parallelism could be added without changing what batch mode produces. See
{doc}`parallelism`.
