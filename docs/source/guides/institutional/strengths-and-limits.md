# Strengths, weaknesses, and how to check them

A page of claims about a project, written by that project, is worth very little on its own. What
makes this one worth reading is that almost every line below names the command that would prove it
wrong.

## Where the project is strong

**The method is inspectable end to end.** A number in the output came from a page of a PDF, read by
a named format, under a recipe you can read. Nothing is a model whose behaviour has to be inferred
from its outputs.

**Support grows without touching the engine.** A new report layout is a new format in somebody
else's repository. The part that must not change does not change in order to cover more issuers —
which is what usually rots this kind of tool.

**Claims are attributable and hash-bound.** A grant says *this person, under this published
methodology, vouched for these exact bytes*. Change the file and the grant is invalid, deliberately.
{doc}`why-trust-a-grant` is what that does and does not establish.

**The project measures itself and publishes the measurements.** Test coverage, documentation
coverage, lint scores, how much of the repository anyone has vouched for — all of it is generated,
all of it is re-runnable, and a commit can be refused over it. {doc}`../devops/index` is the
machinery; {doc}`../../dev/ci-report/index` is what it found last time.

**Failures are local.** The page is the unit of work, so a page the engine cannot read is a page,
not a run. {doc}`../../reference/design/pages` is the assumption and what it costs.

## Where the project is weak

**Extraction from documents made for human eyes is not exact, and never will be.** The project's
own position is that any tool claiming otherwise is misrepresenting itself —
{doc}`../../overview/trust`.

**Coverage is whatever people have written formats for.** There is no coverage of an issuer nobody
has cared about yet, and no way to tell from the output that a report *would* have been
misread — only that it was not read.

**A grant is a statement by a person, not a proof.** It says somebody applied a named protocol in
good faith. It does not say they were right, and the methodologies say so themselves.

**The published figures describe one run on one machine.** Every generated page carries that
sentence, because a coverage number measured elsewhere is a fact about elsewhere.

**Parts of it are designed and not built.** That is stated rather than hidden:
{doc}`../../reference/design/limits` lists what is an accepted limit, what is planned, and what is
simply broken. A project that keeps such a list is easier to trust than one that does not, and it is
also an admission.

**There is no continuous integration running right now.** The pipeline exists and is stopped; the
gate is local. {doc}`../devops/index` says so plainly.

## How to check any of it yourself
| Claim | Command that settles it |
|---|---|
| the grants say what the site says | `freeports-validate check-grants` |
| the coverage and lint figures are current | `freeports-dev ci-check` |
| a format reads a given report the way it should | `freeports-dev test`, against that repository |
| a figure in a report came from where it says | `freeports-dev inspect-page` |

Every generated page on this site repeats that invitation, in those words, for the same reason: a
figure you cannot re-derive is a figure you are being asked to take on faith.
