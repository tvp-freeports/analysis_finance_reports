# {title}

<!-- freeports-validate:begin -->
<!-- freeports-validate:end -->

---

Everything between the two markers above is **generated**, and is replaced whole every time the
report is refreshed — so nothing written between them survives. It is the same view as
`{mirrors}`, for the whole repository at once instead of for one file, one contributor or one
methodology.

This repository's `pre-commit` hook rewrites it. By hand:

```sh
freeports-validate report --format markdown --table {table} --out validation/report/{table}.md
```

[← the repository's README](../../README.md) · [the other tables](.)
