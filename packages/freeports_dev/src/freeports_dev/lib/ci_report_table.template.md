# {title}

<!-- freeports-dev:begin -->
<!-- freeports-dev:end -->

---

Everything between the two markers above is **generated**, and is replaced whole every time the
report is refreshed — so nothing written between them survives. {about}

This repository's `pre-commit` hook rewrites it. By hand:

```sh
freeports-dev ci-report --format markdown --table {table} --out ci/report/{table}.md
```

The figures are the ones `freeports-dev ci-check` gates on. Neither this page nor the command that
writes it can refuse a commit — only `ci-check` can, and only on a `prod` branch.

[← the repository's README](../../README.md) · [the other tables](.) · [as one page](index.html)
