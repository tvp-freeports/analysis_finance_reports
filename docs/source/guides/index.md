# Guides, by who you are

This is the audience axis of the documentation. Each section below is written for one figure, opens
with the loop that figure actually works in, and keeps its deep material in its own `advanced`
pages. Where two figures need the same fact, neither repeats it: both point at
{doc}`../reference/index`.

```{important}
**A project is shaped by the people who turn up, not the other way round.** The figures below are
how these pages happen to be arranged today — a record of the work people have already done here,
not a list of the work that counts. They are soft at the edges and certainly incomplete.

So do not look for yourself in the table and conclude you are missing. Take a piece of one section
and a piece of another, bring an expertise none of them anticipated, or arrive with a background
this project has never had — that is the case in which everyone gains most. A contribution these
pages have no section for is a good sign about the project and a gap in the documentation; the
section gets written afterwards, which is how most of the ones below came to exist.
```

```{toctree}
:maxdepth: 2
:hidden:

user/index
formats/index
input-db/index
engine/index
grants/index
docs/index
i18n/index
devops/index
institutional/index
```

## Where might you start?
| If you | A good place to start | It assumes |
|---|---|---|
| want tables out of a report | {doc}`user/index` | nothing but a shell |
| want to support a report layout nobody supports yet | {doc}`formats/index` | you can read a PDF and edit a spreadsheet |
| want to change which companies a run looks for | {doc}`input-db/index` | you can edit CSV |
| want to change the extraction engine itself | {doc}`engine/index` | Rust, and a toolchain |
| want to vouch for a file under a published methodology | {doc}`grants/index` | GnuPG, and that you have read the file |
| want to write or reorganise these pages | {doc}`docs/index` | Sphinx, and the rules on that page |
| want to translate them | {doc}`i18n/index` | gettext |
| own the checks that refuse a commit | {doc}`devops/index` | `make`, and a willingness to argue about thresholds |
| want to know whether to believe any of it | {doc}`institutional/index` | nothing at all |

The first three of those happen in **different repositories** — a formats repository and an input
database are maintained separately from the engine, by whoever wants to. That is the same decision
twice: coverage should grow without anybody touching the part that must not change.
{doc}`../overview/the-repositories` is the map.

## The three kinds of page you will meet
**A guide page** tells you how to do something, in order, in the repository you are in. It is the
kind of page you are reading now.

**A reference page** answers "what does this option do" for every option, without caring who is
asking. It lives under {doc}`../reference/index` and is linked from every guide that needs it.

**A generated page** is written by a command, not by a person — the two API references, the
coverage of the grants, the last gate run. It lives under {doc}`../generated/index`, and every one
of them says which command to run to check it yourself.
