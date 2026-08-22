"""Serialization utilities for test fixtures.

Converts PdfBlock, TextBlock, Promise, Enums, Pydantic models, and related
objects to/from JSON-serializable structures. Uses Pydantic internally for
standard serialization and importlib-based auto-discovery for enums.

No manual type registration required — enum classes are resolved at
deserialization time via importlib.
"""

import importlib
import json
from datetime import date
from enum import Enum
from typing import Any, Optional

from pydantic import BaseModel, ConfigDict

from freeports import _native
from freeports._internals.commons.consts import (
    Currency,
    SfdrArticle,
    FinancialInstrument,
)

PdfBlock = _native.core.PdfBlock
TextBlock = _native.core.TextBlock
Promise = _native.core.Promise

# `Currency`/`SfdrArticle`/`FinancialInstrument` are Rust pyclasses (see
# packages/freeports_engine/src/core/consts.rs), not `enum.Enum` subclasses, so `isinstance(v,
# Enum)` alone no longer recognizes them. `_enum_to_tag`/`_tag_to_enum` work unchanged for them
# (they expose `.name`, `__module__`/`__qualname__`, and bracket lookup `cls[name]`) — they just
# need to be checked for explicitly alongside `Enum`.
_ENUM_LIKE_TYPES = (Enum, Currency, SfdrArticle, FinancialInstrument)


def _is_rust_model(obj: Any) -> bool:
    """True for a Rust pyclass output class (e.g. `FundRename`/`FundMerge`, see
    ``packages/freeports_engine/src/core/fund_change_name.rs``) that plays the same role a
    ``pydantic.BaseModel`` subclass used to: it exposes ``__rust_model_fields__`` (a tuple of
    *all* field names, in the spirit of ``BaseModel.model_fields`` — not the CSV-export alias
    names, and not affected by any ``exclude=True``) so the generic ``__pydantic__`` tag scheme
    below can freeze/thaw it without a per-class branch.
    """
    return hasattr(type(obj), "__rust_model_fields__")


class SerializationError(Exception):
    """Raised when an object cannot be serialized or deserialized."""


# ---------------------------------------------------------------------------
# Enum auto-discovery (replaces TypeRegistry — no manual registration)
# ---------------------------------------------------------------------------


def _enum_to_tag(e: Enum) -> str:
    """Serialize an Enum to a string tag: module:QualName:VALUE."""
    return f"{type(e).__module__}:{type(e).__qualname__}:{e.name}"


def _tag_to_enum(tag: str) -> Enum:
    """Deserialize a string tag back to an Enum instance.

    Uses importlib to resolve the class at deserialization time.
    No registration required.
    """
    module_path, qualname, value = tag.rsplit(":", 2)
    mod = importlib.import_module(module_path)
    cls = getattr(mod, qualname)
    return cls[value]


# ---------------------------------------------------------------------------
# Internal Pydantic models — used only for serialization
# ---------------------------------------------------------------------------


class _PromiseModel(BaseModel):
    """Serialization model for Promise objects."""

    model_config = ConfigDict(frozen=True)

    id: str

    def to_promise(self) -> Promise:
        return Promise(self.id)

    @classmethod
    def from_promise(cls, p: Promise) -> "_PromiseModel":
        return cls(id=str(p))


class _PdfBlockModel(BaseModel):
    """Serialization model for PdfBlock.

    ``type_block`` is a plain string (see ``core/classes.py`` for why) — it used to be
    ``type_block_tag``, a ``module:QualName:MEMBER`` path round-tripped through
    ``_enum_to_tag``/``_tag_to_enum`` (dynamic re-import). None of that machinery is needed for a
    plain string; it's serialized as-is.
    """

    model_config = ConfigDict(arbitrary_types_allowed=True)

    type_block: str
    metadata: dict
    content: Optional[str | _PromiseModel] = None

    def to_pdf_block(self) -> PdfBlock:
        return PdfBlock(
            type_block=self.type_block,
            metadata=_thaw_dict(self.metadata),
            text=_PromiseModel.to_promise(self.content)
            if isinstance(self.content, _PromiseModel)
            else self.content,
        )

    @classmethod
    def from_pdf_block(cls, blk: PdfBlock) -> "_PdfBlockModel":
        return cls(
            type_block=blk.type_block,
            metadata=_freeze_dict(blk.metadata),
            content=_PromiseModel.from_promise(blk.content)
            if isinstance(blk.content, Promise)
            else blk.content,
        )


class _TextBlockModel(BaseModel):
    """Serialization model for TextBlock and subclasses.

    ``type_block`` is a plain string — see ``_PdfBlockModel``'s docstring for why this no longer
    needs the tag/importlib round-trip.
    """

    model_config = ConfigDict(arbitrary_types_allowed=True)

    subtype_tag: Optional[str] = None
    type_block: str
    metadata: dict
    content: str | _PromiseModel
    pdf_block: Optional[dict] = None

    def to_text_block(self) -> TextBlock:
        # `subtype_tag` is no longer resolved to reconstruct a distinct Python type: `TextBlock`
        # is now a Rust pyclass (see `packages/freeports_engine/src/core/classes.rs`) and is
        # never subclassed any more (the three format-level "subclasses" that used to exist —
        # `StandardFundTextBlock` & co. — became thin factories returning a real `TextBlock`
        # instead, see `formats/utils/text_filter/standard_txt_blks.py`). `cls.__new__(cls)`
        # (the old way of building an object without calling `__init__`) doesn't work for a PyO3
        # pyclass, and there is nothing left to resolve `subtype_tag` *to* — verified that
        # nothing anywhere does `isinstance(x, StandardFundTextBlock)`-style type checks, so
        # always reconstructing a plain `TextBlock` (even for a fixture recorded before this
        # change, which may still have a non-null `subtype_tag`) changes nothing observable.
        blk = TextBlock.from_content(
            self.type_block,
            _thaw_dict(self.metadata),
            _PromiseModel.to_promise(self.content)
            if isinstance(self.content, _PromiseModel)
            else self.content,
        )
        blk.pdf_block = (
            _PdfBlockModel(**self.pdf_block).to_pdf_block() if self.pdf_block else None
        )
        return blk

    @classmethod
    def from_text_block(cls, blk: TextBlock) -> "_TextBlockModel":
        subtype = None
        tname = type(blk).__qualname__
        if tname != "TextBlock":
            subtype = f"{type(blk).__module__}:{tname}"
        return cls(
            subtype_tag=subtype,
            type_block=blk.type_block,
            metadata=_freeze_dict(blk.metadata),
            content=_PromiseModel.from_promise(blk.content)
            if isinstance(blk.content, Promise)
            else blk.content,
            pdf_block=(
                _PdfBlockModel.from_pdf_block(blk.pdf_block).model_dump()
                if blk.pdf_block
                else None
            ),
        )


# ---------------------------------------------------------------------------
# Recursive helpers for metadata (handles nested enums, promises, sets)
# ---------------------------------------------------------------------------


def _freeze_dict(metadata: dict) -> dict:
    """Recursively freeze a metadata dict, converting all values to JSON-safe form.

    Uses the same tagging scheme as to_serializable() but operates on dict
    values directly (no top-level object tagging).
    """
    return _freeze_value(metadata)


def _thaw_dict(frozen: dict) -> dict:
    """Recursively thaw a frozen metadata dict back to original form."""
    return _thaw_value(frozen)


def _freeze_value(v: Any) -> Any:
    """Recursively convert a value to JSON-safe form."""
    if v is None:
        return None
    if isinstance(v, (bool, int, float)):
        return v
    if isinstance(v, str):
        return v
    if isinstance(v, Promise):
        return {
            "__promise__": True,
            "id": str(v),
            "strict": v.strict,
            "multiple": v.multiple,
        }
    if isinstance(v, _ENUM_LIKE_TYPES):
        return {"__enum__": _enum_to_tag(v)}
    if isinstance(v, (set, frozenset)):
        return {"__set__": sorted(_freeze_value(x) for x in v)}
    if isinstance(v, (list, tuple)):
        return [_freeze_value(x) for x in v]
    if isinstance(v, dict):
        return {str(k): _freeze_value(val) for k, val in v.items()}
    if isinstance(v, date):
        return {"__date__": v.isoformat()}
    # Pydantic models in metadata (rare but possible), and Rust-backed output classes that play
    # the same role (see `_is_rust_model`).
    if isinstance(v, BaseModel) or _is_rust_model(v):
        field_names = (
            v.model_fields
            if isinstance(v, BaseModel)
            else type(v).__rust_model_fields__
        )
        fields = {}
        for field_name in field_names:
            val = getattr(v, field_name, None)
            fields[field_name] = _freeze_value(val)
        return {
            "__pydantic__": {
                "class": f"{type(v).__module__}:{type(v).__qualname__}",
                "data": fields,
            }
        }
    raise SerializationError(f"Cannot freeze {type(v).__qualname__}: {v!r}")


def _thaw_value(v: Any) -> Any:
    """Recursively restore a value from its frozen form."""
    if v is None:
        return None
    if isinstance(v, (bool, int, float)):
        return v
    if isinstance(v, str):
        return v
    if isinstance(v, list):
        return [_thaw_value(x) for x in v]
    if isinstance(v, dict):
        if v.get("__promise__") is True and "id" in v:
            return Promise(
                v["id"],
                strict=v.get("strict", False),
                multiple=v.get("multiple", False),
            )
        if "__enum__" in v:
            return _tag_to_enum(v["__enum__"])
        if "__date__" in v:
            return date.fromisoformat(v["__date__"])
        if "__set__" in v:
            return {_thaw_value(x) for x in v["__set__"]}
        if "__pydantic__" in v:
            mod_path, cls_name = v["__pydantic__"]["class"].rsplit(":", 1)
            mod = importlib.import_module(mod_path)
            cls = getattr(mod, cls_name)
            resolved = {
                k: _thaw_value(val) for k, val in v["__pydantic__"]["data"].items()
            }
            if hasattr(cls, "model_validate"):
                return cls.model_validate(resolved)
            return cls(**resolved)
        return {k: _thaw_value(val) for k, val in v.items()}
    raise SerializationError(f"Cannot thaw {type(v).__qualname__}: {v!r}")


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def to_serializable(obj: Any) -> Any:
    """Convert an object to a JSON-serializable structure.

    Uses type tags for domain objects so they can be losslessly reconstructed
    by from_serializable().

    Parameters
    ----------
    obj : Any
        The object to serialize. Must be one of: PdfBlock, TextBlock, Promise,
        Enum, Pydantic BaseModel, or JSON-native types.

    Returns
    -------
    Any
        A JSON-serializable structure.

    Raises
    ------
    SerializationError
        If the object type is not supported.
    """
    if obj is None:
        return None

    if isinstance(obj, bool):
        return obj
    if isinstance(obj, (int, float)):
        return obj
    if isinstance(obj, str):
        return obj

    if isinstance(obj, Promise):
        return {
            "__promise__": True,
            "id": str(obj),
            "strict": obj.strict,
            "multiple": obj.multiple,
        }

    if isinstance(obj, _ENUM_LIKE_TYPES):
        return {"__enum__": _enum_to_tag(obj)}

    if isinstance(obj, date):
        return {"__date__": obj.isoformat()}

    if isinstance(obj, PdfBlock):
        model = _PdfBlockModel.from_pdf_block(obj)
        partial = model.model_dump()
        partial["__pdf_block__"] = True
        return partial

    if isinstance(obj, TextBlock):
        model = _TextBlockModel.from_text_block(obj)
        partial = model.model_dump()
        partial["__text_block__"] = True
        return partial

    if isinstance(obj, BaseModel) or _is_rust_model(obj):
        model_type = f"{type(obj).__module__}:{type(obj).__qualname__}"
        field_names = (
            obj.model_fields
            if isinstance(obj, BaseModel)
            else type(obj).__rust_model_fields__
        )
        fields: dict[str, Any] = {}
        for field_name in field_names:
            value = getattr(obj, field_name, None)
            fields[field_name] = to_serializable(value)
        return {
            "__pydantic__": {
                "class": model_type,
                "data": fields,
            }
        }

    if isinstance(obj, (set, frozenset)):
        return {"__set__": sorted(to_serializable(v) for v in obj)}

    if isinstance(obj, (list, tuple)):
        return [to_serializable(v) for v in obj]

    if isinstance(obj, dict):
        return {str(k): to_serializable(v) for k, v in obj.items()}

    raise SerializationError(f"Cannot serialize {type(obj).__qualname__}: {obj!r}")


def from_serializable(data: Any) -> Any:
    """Reconstruct an object from a serialized structure.

    Parameters
    ----------
    data : Any
        A structure produced by to_serializable().

    Returns
    -------
    Any
        The reconstructed object.

    Raises
    ------
    SerializationError
        If the data contains unknown type tags.
    """
    if data is None:
        return None

    if isinstance(data, bool):
        return data
    if isinstance(data, (int, float)):
        return data
    if isinstance(data, str):
        return data

    if isinstance(data, list):
        return [from_serializable(v) for v in data]

    if isinstance(data, dict):
        if data.get("__promise__") is True and "id" in data:
            return Promise(
                data["id"],
                strict=data.get("strict", False),
                multiple=data.get("multiple", False),
            )

        if "__enum__" in data:
            return _tag_to_enum(data["__enum__"])

        if "__date__" in data:
            return date.fromisoformat(data["__date__"])

        if "__set__" in data:
            return {from_serializable(v) for v in data["__set__"]}

        if "__pydantic__" in data:
            mod_path, cls_name = data["__pydantic__"]["class"].rsplit(":", 1)
            mod = importlib.import_module(mod_path)
            cls = getattr(mod, cls_name)
            resolved_data = {
                k: from_serializable(v) for k, v in data["__pydantic__"]["data"].items()
            }
            if hasattr(cls, "model_validate"):
                return cls.model_validate(resolved_data)
            return cls(**resolved_data)

        if data.pop("__pdf_block__", False):
            return _PdfBlockModel(**data).to_pdf_block()

        if data.pop("__text_block__", False):
            return _TextBlockModel(**data).to_text_block()

        return {k: from_serializable(v) for k, v in data.items()}

    raise SerializationError(f"Cannot deserialize {type(data).__qualname__}: {data!r}")


def dumps(obj: Any, indent: Optional[int] = 2) -> str:
    """Serialize an object to a JSON string.

    Parameters
    ----------
    obj : Any
        The object to serialize.
    indent : Optional[int]
        JSON indentation level. Default 2.

    Returns
    -------
    str
        JSON string representation.
    """
    return json.dumps(to_serializable(obj), indent=indent, ensure_ascii=False)


def loads(data: str) -> Any:
    """Deserialize an object from a JSON string.

    Parameters
    ----------
    data : str
        JSON string produced by dumps().

    Returns
    -------
    Any
        The reconstructed object.
    """
    return from_serializable(json.loads(data))


def dump(obj: Any, fp) -> None:
    """Serialize an object to a JSON file.

    Parameters
    ----------
    obj : Any
        The object to serialize.
    fp : file-like
        Writable file handle.
    """
    json.dump(to_serializable(obj), fp, indent=2, ensure_ascii=False)


def load(fp) -> Any:
    """Deserialize an object from a JSON file.

    Parameters
    ----------
    fp : file-like
        Readable file handle.

    Returns
    -------
    Any
        The reconstructed object.
    """
    return from_serializable(json.load(fp))
