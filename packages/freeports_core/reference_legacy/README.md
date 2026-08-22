# `reference_legacy/`

Reference-only archive of dead Python code that was moved out of the live `python/freeports/`
tree during the maturin-idiomatic restructure (see
`analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md`, §6). This directory
is a sibling of `python/` and `src/`, **not** nested inside `python/` — it is never picked up by
maturin's `python-source` packaging and is never shipped in a built wheel.

Every symbol archived here was already marked `_legacy_*` (or, in one case, confirmed dead via a
workspace-wide grep with zero live callers) in its original live file, confirmed to have zero
callers anywhere in this repo, `freeports_dev`, or `analysis_finance_reports_formats` at the time
of the move. Docstrings are preserved verbatim from the live tree — no summarizing or
paraphrasing — since the user wants them kept as future inspiration for Rust rustdoc comments
when the equivalent logic is ported.

This tree is expected to eventually be deleted once the Rust migration is far enough along that
nobody needs to consult these original Python bodies anymore.

## Source files these bodies came from

- `python/freeports/_internals/formats/utils/deserialize/cast.py` — the 11 `_legacy_*` casting
  functions (`_legacy_perc_to_float`, `_legacy_force_numeric`, `_legacy_to_float`,
  `_legacy_to_int`, `_legacy_to_str`, `_legacy_to_currency`, `_legacy_to_date`,
  `_legacy_to_int_en_month`, `_legacy_to_date_with_en_month`, `_legacy_to_int_it_month`,
  `_legacy_to_date_with_it_month`).
- `python/freeports/_internals/commons/i18n.py` — `_legacy_load_translation`.
- `python/freeports/_internals/commons/enum_utils.py` — `_legacy_flag_from_string`.
- `python/freeports/_internals/cli/cmd.py` — `_legacy_cmd`.
- `python/freeports/_internals/cli/main.py` — `_legacy_main`, `batch_job_confs`, `_output_file`,
  `NoPDFormatDetected`, and the trailing `if __name__ == "__main__":` block (none of these were
  `_legacy_`-prefixed in the source, but each was confirmed to have zero live callers via a
  workspace-wide grep — see the plan's §6c for the per-symbol reasoning).
