"""Archived dead code, moved out of `python/freeports/_internals/commons/enum_utils.py` during
the maturin-idiomatic restructure session (2026-08-21) — see
`analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md`, §6b. Reference-only,
never packaged (see this directory's own `reference_legacy/README.md`). Docstring below is
preserved verbatim from the live tree.

``_legacy_flag_from_string`` was the original (pre-Rust-port) AST-based implementation of
``flag_from_string``'s expression evaluation, superseded by
``freeports._native.core.evaluate_flag_expression`` (`src/core/flag_expr.rs`); the live file kept
it as dead code pending this move.
"""

import ast
import operator
from enum import Flag
from typing import Type, Optional, Union
import pandas as pd
from freeports.i18n import _


def _legacy_flag_from_string(
    expression: Optional[Union[str, list]], cls: Type[Flag]
) -> Flag:
    """Dead code: the original AST-based implementation, superseded by the Rust expression
    evaluator imported above. Kept until the migration is far enough along to delete it.
    """
    bin_ops = {
        ast.BitAnd: operator.and_,
        ast.BitOr: operator.or_,
        ast.BitXor: operator.xor,
    }
    unary_ops = {ast.Invert: operator.invert}

    def _from_ast(node: ast.AST, flag_cls: Type[Flag]) -> Flag:
        if isinstance(node, ast.Expression):
            return _from_ast(node.body, flag_cls)
        if isinstance(node, ast.BinOp):
            left = node.left
            right = node.right
            op = type(node.op)
            if op in bin_ops:
                return bin_ops[op](
                    _from_ast(left, flag_cls), _from_ast(right, flag_cls)
                )
            raise ValueError(_("Binary operation {} not supported").format(op))
        if isinstance(node, ast.UnaryOp):
            operand = node.operand
            op = type(node.op)
            if op in unary_ops:
                return unary_ops[op](_from_ast(operand, flag_cls))
            raise ValueError(_("Unary operation {} not supported").format(op))
        if isinstance(node, ast.Name):
            name = node.id.upper()
            if hasattr(flag_cls, name):
                return getattr(flag_cls, name)
            raise ValueError(_("Invalid flag {}").format(name))
        raise ValueError(_("Unsupported AST node: {}").format(type(node)))

    if isinstance(expression, list):
        expression = " | ".join(expression)
        return _legacy_flag_from_string(expression, cls)
    if pd.isna(expression):
        return None
    if isinstance(expression, str):
        if expression.strip() == "":
            return cls(0)
        expression = ast.parse(expression, mode="eval")
        return _from_ast(expression, cls)
    raise ValueError(_("Flags should be specified with list or string expression"))
