# Methodology sources

A grant names a methodology, and a methodology is a **document at an address**. This page is how `freeports-validate` is told which addresses to trust and how it resolves a name into a body of text. Why the mechanism is shaped this way — and why the source is deliberately not recorded in the grant — is {doc}`../../guides/institutional/why-trust-a-grant`.

## What a source is

A grant is a claim made **under a text**, so the tool has to be able to read that text, and it does
not carry one. Methodology pages are resolved from **sources** you configure, and the hash your
document records is the hash of whatever your sources gave you.

That is a deliberate trade, and it is worth stating both halves. Were the pages to travel inside the
installed package, upgrading `freeports-validate` would silently change what every grant in every
repository referred to. Instead the text is a document at an address you chose — and the mirror
image of that freedom is that whoever publishes the address can invalidate your grants by editing
the page. {doc}`../../guides/institutional/why-trust-a-grant` says why that is the right way round.

**The source is not recorded in your document.** A validation document stores a methodology's *name*
and its *sha256*, and nothing about where either came from. The source is a contract between the
person who granted and the person who verifies, held in the configuration each of them writes. The
cost is that a hash mismatch is ambiguous — the page may have been rewritten, or the two of you may
simply be reading two publications of the same methodology — and `sources` below is where that cost
is paid back.

## The setting

| Tier | Spelling | Type |
|---|---|---|
| configuration file | `validate.sources` | a YAML **list** of strings |
| environment | `FREEPORTS_VALIDATE_SOURCE` | **one** source — the whole value, never split |
| command line | `--source URI`, `-s URI` | repeatable |

With no tier naming anything, the default is the published documentation:
`https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt`.

Two rules about the list are worth learning once:

**The tiers do not merge.** The strongest tier that names any source provides the whole list, so a
command line with `-s` ignores the configuration file's list entirely. Merging would make *which
text did this hash come from* unanswerable from any one place. The case that wants two sources at
once — the published documentation **plus** your own methodologies — is served by listing both in
the same tier, which is why the file tier takes a list.

**The environment carries one source, and its value is never split.** Every separator one might pick
— comma, colon, space, semicolon — is a legal character in a URI or a path, so splitting would turn
one source you meant into two that do not exist. This is the same rule `FREEPORTS_TARGET_LIST`
follows, for the same reason.

**Order is priority.** A name is resolved from the first source that offers it; the rest are
shadowed, and `sources` says so.

## The grammar of a source

A source is a pattern containing **exactly one `*`**. The `*` stands for the methodology's relative
name, which may itself contain a `/`; everything before it is the base and everything after it is
the extension.

```text
https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
https://github.com/tvp-freeports/analysis_finance_reports/blob/main/docs/source/validation/*.rst
file:///home/me/my-methodologies/*.rst
../my-methodologies/*.rst
```

| What is being resolved | `*` becomes |
|---|---|
| the general methodology — your document's `version` field | `general_methodology` |
| a methodology named `basic check` | `methodologies/basic_check` |

| Rule | |
|---|---|
| zero, or more than one, `*` | a configuration error naming the offending source — it is not guessable which half is the base |
| a `github.com/<owner>/<repo>/blob/<ref>/<path>` URL | rewritten to `raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`, because fetching the `blob` page would hash a web page rather than a document, and would do it silently. Both spellings are correct input |
| `http://` | accepted, and warned about once per run: the hash is what compensates for an unauthenticated fetch, but you should know which kind you are doing |
| a bare relative path | accepted. In a **configuration file** it is resolved against the file's own directory, because that file is searched for and one line in it is read from many working directories; on the command line and in the environment it is relative to the shell you typed it in |

Sphinx publishes reStructuredText sources under `_sources/` with a `.txt` suffix appended, which is
why the default pattern ends in `.rst.txt` rather than `.rst`.

## Fetching, the cache, and being offline

Pages are fetched with `curl -fsSL` and cached, content-addressed, under
`${XDG_CACHE_HOME:-$HOME/.cache}/freeports-validate/<sha256 of the URI>/<sha256 of the body>`. Every
distinct body ever seen for a URI is kept, and a `latest` symlink names the most recent one.

**Online, the cache is a fallback and never a shortcut.** A run with a network always fetches. A
cache consulted first would quietly pin a methodology to whatever was fetched the day you first saw
it — and the published text changing is precisely the event this whole mechanism exists to notice.

| Situation | What happens |
|---|---|
| the fetch succeeds | the fetched body is hashed, and cached |
| the fetch fails, no `--offline` | the check is **inconclusive** — not matching and not mismatched. `check-grants` exits `3` over it: nothing failed, and not everything was checked |
| the fetch fails, `--offline` given | the newest cached body is used, and every line that depended on it says so |
| `--offline`, nothing cached | reported as unreachable |

`--offline` is a setting like any other: `validate.offline`, `FREEPORTS_VALIDATE_OFFLINE`,
`--offline`. A machine can be permanently without egress — an air-gapped build, a runner with no
network — which is a property of the machine, written down once, rather than something to remember
on every run.

## Seeing what your configuration resolves

`sources` is the answer to *why does my hash differ from yours*. Two people can compare its output
and see in one line which of the two possible causes it was. It needs no key.

```console
$ freeports-validate sources
ℹ️  Methodology sources, in priority order
==========================================
  1. https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt

A name is resolved from the first source that offers it.

ℹ️  What each name resolves to here
==========================================

the general methodology
  as:     general_methodology
  from:   https://docs.freeports.org/en/stable/_sources/validation/general_methodology.rst.txt
  sha256: d7f23dc8ad3014b4f9b7a0e550f24c83531559c46e91d28d25706c323b890323

basic check
  as:     methodologies/basic_check
  from:   https://docs.freeports.org/en/stable/_sources/validation/methodologies/basic_check.rst.txt
  sha256: eb4b396bf453fbf6273d7cca7ef0fc7fac925c03c20dd98bcae054d75a148f37
```

It reports the general methodology, every methodology adopted by a document in this repository, and
anything you name as an argument. It does **not** offer a catalogue: a remote source cannot be
enumerated at all — there is no way to ask a URL what else it serves — so a listing would silently
mean *the local ones* while looking like *all of them*. Where one source shadows another for the
same name, it says which, which is the fix for the failure where you edit a page, nothing changes,
and nothing anywhere says why.
