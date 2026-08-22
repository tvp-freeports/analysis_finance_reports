"""Small live pieces of the pre-Rust-port `Algorithm`/`PipelinesBundle` definitions module.

``PipelinesBundle`` and ``Algorithm`` are now implemented in Rust — see
``packages/freeports_engine/src/pipeline.rs`` and
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md`` (Phase B). Every real
consumer of the pure-alias names this module used to re-export (``Algorithm``/``PipelinesBundle``,
both literally ``freeports_engine.core.X``, now ``freeports._native.core.X`` since the
``freeports_engine`` -> ``freeports._native`` maturin-idiomatic restructure — see
``analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md``) now imports them
directly from ``freeports._native.core`` — see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md`` Decision 3. This
module now only holds the two small classes that had no Rust counterpart and no
``_legacy``/``_Legacy`` dead-code twin: ``LogFormatterWithPage`` and ``PoolWorkersSettings``.
Everything else that used to live here (the ``_LegacyAlgorithm``/``_LegacyPipelinesBundle``/
``_LegacyPageClassificationPipeline`` dead code, and the live-but-now-redundant
``Algorithm``/``PipelinesBundle`` aliases) was removed in that same consolidation.
"""

import logging as log
from typing import Optional


class LogFormatterWithPage(log.Formatter):
    """Log formatter that adds page number context to log messages.

    This formatter wraps an existing formatter and inserts page number
    information into formatted log records.

    Attributes
    ----------
    _parent_fmt : log.Formatter
        The original formatter to wrap
    page : Optional[int]
        Current page number for context
    """

    def __init__(self, old_formatter: log.Formatter):
        """Initialize the LogFormatterWithPage with a reference formatter.

        Parameters
        ----------
        old_formatter : log.Formatter
            The formatter to use as a base for formatting

        Notes
        -----
        The page number is dynamically inserted into log messages
        by replacing colons with page context information.
        """
        super().__init__()
        self._parent_fmt = old_formatter
        self.page: Optional[int] = None

    def format(self, record: log.LogRecord) -> str:
        """Format a log record with page number context.

        Parameters
        ----------
        record : log.LogRecord
            The log record to format

        Returns
        -------
        str
            Formatted log message with page number inserted
        """
        string = self._parent_fmt.format(record).replace(":", f"{{pag. {self.page}}}:")
        return string


class PoolWorkersSettings:
    """Configuration for multiprocessing pool worker counts.

    Attributes
    ----------
    documents : int
        Number of worker processes for document-level parallelism.
    pages : int
        Number of worker processes for page-level parallelism.
    pipelines : int
        Number of worker processes for pipeline-level parallelism.
    pipes : int
        Number of worker processes for pipe-level parallelism.
    """

    documents: int
    pages: int
    pipelines: int
    pipes: int
