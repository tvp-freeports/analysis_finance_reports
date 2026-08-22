"""Casting utilities for deserializing string data into typed Python values.

The actual casting logic is now implemented in Rust — see
``packages/freeports_engine/src/core/cast.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. The names below delegate to
that implementation; the original Python bodies (previously kept further down in this file,
renamed with a ``_legacy_`` prefix, as dead code) were moved to
``reference_legacy/_internals/formats/utils/deserialize/cast.py`` during the maturin-idiomatic
restructure (see `agent-memory/maturin-idiomatic-restructure-plan.md`, §6b) — reference-only,
never packaged.

The Python originals call ``logger.warning(...)`` (translated via ``_()``) as a side effect on
the *success* path, when a value needs a lossy/forced cast (``to_float``/``to_int``) or when a
percentage sign forces normalization despite ``norm=False`` (``perc_to_float``). These warnings
are load-bearing: several format fixtures' ``.log.csv`` audit files assert on them. The Rust
port only computes the result; ``to_float``, ``to_int``, and ``perc_to_float`` below are thin
Python wrappers that reproduce the warning side effects (using
``freeports._native.core.is_numeric_shape`` / ``normalize_word``) before delegating the actual
computation to Rust.
"""

import logging

from freeports import _native
from freeports.i18n import _

normalize_string = _native.core.normalize_string
normalize_word = _native.core.normalize_word

logger = logging.getLogger(__name__)


def _log_forced_cast_if_needed(data: str) -> None:
    normalized = normalize_word(data)
    if not _native.core.is_numeric_shape(normalized):
        logger.warning(
            _("Trying to cast to number but found '%s' - forcing cast"), normalized
        )


def to_float(data: str) -> float:
    _log_forced_cast_if_needed(data)
    return _native.core.to_float(data)


def to_int(data: str) -> int:
    _log_forced_cast_if_needed(data)
    return _native.core.to_int(data)


def perc_to_float(perc: str, norm: bool = True) -> float:
    normalized = normalize_word(perc)
    if "%" in normalized:
        normalized = normalize_word(normalized.replace("%", ""))
        if not norm:
            logger.warning(
                _(
                    "Found percentage symbol '%' but `norm` is False - forcing normalization"
                )
            )
    _log_forced_cast_if_needed(normalized)
    return _native.core.perc_to_float(perc, norm)


to_str = _native.core.to_str
to_currency = _native.core.to_currency
to_date = _native.core.to_date
to_int_en_month = _native.core.to_int_en_month
to_date_with_en_month = _native.core.to_date_with_en_month
to_int_it_month = _native.core.to_int_it_month
to_date_with_it_month = _native.core.to_date_with_it_month

__all__ = [
    "perc_to_float",
    "to_float",
    "to_int",
    "to_str",
    "to_currency",
    "to_date",
    "to_int_en_month",
    "to_date_with_en_month",
    "to_int_it_month",
    "to_date_with_it_month",
]
