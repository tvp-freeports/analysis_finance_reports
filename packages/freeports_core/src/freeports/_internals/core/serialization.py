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

from freeports._internals.core.classes import PdfBlock, TextBlock
from freeports._internals.core.promises import Promise


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
    """Serialization model for PdfBlock."""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    type_block_tag: str
    metadata: dict
    content: Optional[str | _PromiseModel] = None

    def to_pdf_block(self) -> PdfBlock:
        return PdfBlock(
            type_block=_tag_to_enum(self.type_block_tag),
            metadata=_thaw_dict(self.metadata),
            text=_PromiseModel.to_promise(self.content)
            if isinstance(self.content, _PromiseModel)
            else self.content,
        )

    @classmethod
    def from_pdf_block(cls, blk: PdfBlock) -> "_PdfBlockModel":
        return cls(
            type_block_tag=_enum_to_tag(blk.type_block),
            metadata=_freeze_dict(blk.metadata),
            content=_PromiseModel.from_promise(blk.content)
            if isinstance(blk.content, Promise)
            else blk.content,
        )


class _TextBlockModel(BaseModel):
    """Serialization model for TextBlock and subclasses."""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    subtype_tag: Optional[str] = None
    type_block_tag: str
    metadata: dict
    content: str | _PromiseModel
    pdf_block: Optional[dict] = None

    def to_text_block(self) -> TextBlock:
        cls = TextBlock
        if self.subtype_tag:
            mod_path, cls_name = self.subtype_tag.rsplit(":", 1)
            mod = importlib.import_module(mod_path)
            cls = getattr(mod, cls_name)

        type_block = _tag_to_enum(self.type_block_tag)
        blk = cls.__new__(cls)
        blk.type_block = type_block
        blk.metadata = _thaw_dict(self.metadata)
        blk.content = (
            _PromiseModel.to_promise(self.content)
            if isinstance(self.content, _PromiseModel)
            else self.content
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
            type_block_tag=_enum_to_tag(blk.type_block),
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
        return {"__promise__": True, "id": str(v)}
    if isinstance(v, Enum):
        return {"__enum__": _enum_to_tag(v)}
    if isinstance(v, (set, frozenset)):
        return {"__set__": sorted(_freeze_value(x) for x in v)}
    if isinstance(v, (list, tuple)):
        return [_freeze_value(x) for x in v]
    if isinstance(v, dict):
        return {str(k): _freeze_value(val) for k, val in v.items()}
    if isinstance(v, date):
        return {"__date__": v.isoformat()}
    # Pydantic models in metadata (rare but possible)
    if isinstance(v, BaseModel):
        fields = {}
        for field_name in v.model_fields:
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
            return Promise(v["id"])
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
            return cls.model_validate(resolved)
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
        return {"__promise__": True, "id": str(obj)}

    if isinstance(obj, Enum):
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

    if isinstance(obj, BaseModel):
        model_type = f"{type(obj).__module__}:{type(obj).__qualname__}"
        fields: dict[str, Any] = {}
        for field_name in obj.model_fields:
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
            return Promise(data["id"])

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
            return cls.model_validate(resolved_data)

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
