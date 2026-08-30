"""Serializzazione delle fixture di test (JSON <-> oggetti `freeports`).

Porting di `freeports._internals.core.serialization`, che viveva dentro il pacchetto
`freeports`. Il modulo **si sposta qui**, e non riappare come sottomodulo dell'estensione
Rust, per due ragioni: `freeports._internals` non esiste piu' (era il livello di
implementazione in Python puro, che nella riscrittura sta in Rust), e questa e' logica di
*fixture di test*, il cui unico consumatore e' `freeports-dev` — non fa parte dell'API che un
autore di formato usa.

Rispetto all'originale spariscono i modelli Pydantic: gli oggetti da serializzare sono ora
`#[pyclass]` Rust, non `BaseModel`, e i campi si enumerano con `__serialize_fields__()`
(ricavato dalla forma serde dell'entita', vedi `python/output.rs`) invece che con
`model_fields`. Lo schema JSON dei tag e' invariato, cosi' le fixture gia' registrate restano
leggibili.
"""

import importlib
import json
from datetime import date
from enum import Enum
from typing import Any, Optional

from freeports.consts import Currency, FinancialInstrument, SfdrArticle
from freeports.core import PdfBlock, Promise, TextBlock

# `Currency`/`SfdrArticle`/`FinancialInstrument` sono pyclass Rust, non sottoclassi di
# `enum.Enum`, quindi `isinstance(v, Enum)` da solo non le riconosce. Il resto del protocollo che
# serve qui (`.name`, `__module__`/`__qualname__`, e la lookup `cls[NOME]`) ce l'hanno.
_ENUM_LIKE_TYPES = (Enum, Currency, SfdrArticle, FinancialInstrument)


class SerializationError(Exception):
    """Un oggetto non e' serializzabile, o un tag non e' risolvibile."""


def _resolve_class(tag: str):
    """La classe nominata da un tag `modulo:QualName`."""
    module_path, qualname = tag.rsplit(":", 1)
    module = importlib.import_module(module_path)
    try:
        return getattr(module, qualname)
    except AttributeError as exc:
        raise SerializationError(f"unknown class in tag {tag!r}") from exc


def _enum_to_tag(e) -> str:
    return f"{type(e).__module__}:{type(e).__qualname__}:{e.name}"


def _tag_to_enum(tag: str):
    module_path, qualname, name = tag.rsplit(":", 2)
    cls = _resolve_class(f"{module_path}:{qualname}")
    return cls[name]


def _is_entity(obj: Any) -> bool:
    """Vero per uno shim di entita' di `freeports.output`, che sa elencare i propri campi."""
    return hasattr(obj, "__serialize_fields__")


def _promise_tag(p: Promise) -> dict:
    return {
        "__promise__": True,
        "id": p.id,
        "strict": p.strict,
        "multiple": p.multiple,
    }


def _promise_from_tag(data: dict) -> Promise:
    return Promise(
        data["id"],
        strict=data.get("strict", False),
        multiple=data.get("multiple", False),
    )


def _entity_tag(obj: Any) -> dict:
    fields = {
        name: to_serializable(getattr(obj, name, None))
        for name in obj.__serialize_fields__()
    }
    # La chiave resta `__pydantic__` benche' di Pydantic non ci sia piu' nulla: e' il nome che le
    # fixture gia' registrate usano, e cambiarlo le renderebbe tutte illeggibili in cambio di
    # niente.
    return {
        "__pydantic__": {
            "class": f"{type(obj).__module__}:{type(obj).__qualname__}",
            "data": fields,
        }
    }


def _entity_from_tag(data: dict) -> Any:
    cls = _resolve_class(data["class"])
    return cls(**{k: from_serializable(v) for k, v in data["data"].items()})


def _pdf_block_tag(blk: PdfBlock) -> dict:
    return {
        "__pdf_block__": True,
        "type_block": blk.type_block,
        "metadata": to_serializable(blk.metadata),
        "content": to_serializable(blk.content),
    }


def _block_content(data: dict) -> Any:
    """Il contenuto di un blocco, ricostruito.

    Le fixture registrate dal riferimento serializzavano una `Promise` nel campo `content` di un
    blocco **senza** il tag `__promise__`, come `{"id": "..."}` e basta: il modello Pydantic che
    leggeva quel campo lo dichiarava tipizzato (`Optional[str | _PromiseModel]`) e Pydantic ne
    deduceva il tipo dalla forma. Senza Pydantic quella deduzione va rifatta a mano, altrimenti la
    promessa torna come dizionario e il confronto con l'output del motore fallisce su un blocco
    per pagina.
    """
    content = data.get("content")
    if (
        isinstance(content, dict)
        and set(content) <= {"id", "strict", "multiple"}
        and "id" in content
    ):
        return _promise_from_tag(content)
    return from_serializable(content)


def _pdf_block_from_tag(data: dict) -> PdfBlock:
    return PdfBlock(
        data["type_block"],
        from_serializable(data.get("metadata") or {}),
        _block_content(data),
    )


def _text_block_tag(blk: TextBlock) -> dict:
    return {
        "__text_block__": True,
        # `subtype_tag` non e' piu' prodotto: i tre "sottotipi" del riferimento
        # (`StandardFundTextBlock` & co.) sono fabbriche che restituiscono un `TextBlock` normale,
        # quindi non c'e' nessun tipo distinto da ricostruire. La chiave resta accettata in
        # lettura per le fixture vecchie che la contengono.
        "type_block": blk.type_block,
        "metadata": to_serializable(blk.metadata),
        "content": to_serializable(blk.content),
        "pdf_block": _pdf_block_tag(blk.pdf_block)
        if blk.pdf_block is not None
        else None,
    }


def _text_block_from_tag(data: dict) -> TextBlock:
    # Sempre `from_content` + attacco del blocco PDF, mai il costruttore a tre argomenti: quello
    # il contenuto lo **eredita** dal blocco PDF, e qui il contenuto registrato puo' essere stato
    # riscritto dal modulo d'autore dopo la costruzione (succede davvero: ANIMA_SICAV-EN24 e
    # KAIROS-EN23 tolgono un suffisso dal nome del fondo). Ereditandolo si perderebbe la
    # riscrittura, e la fixture tornerebbe diversa da cio' che era stato registrato.
    block = TextBlock.from_content(
        data["type_block"],
        from_serializable(data.get("metadata") or {}),
        _block_content(data),
    )
    pdf_block = data.get("pdf_block")
    if pdf_block:
        block.pdf_block = _pdf_block_from_tag(pdf_block)
    return block


def to_serializable(obj: Any) -> Any:
    """Un oggetto nella forma JSON-safe che `from_serializable` sa ricostruire."""
    if obj is None or isinstance(obj, (bool, int, float, str)):
        return obj
    if isinstance(obj, Promise):
        return _promise_tag(obj)
    if isinstance(obj, _ENUM_LIKE_TYPES):
        return {"__enum__": _enum_to_tag(obj)}
    if isinstance(obj, date):
        return {"__date__": obj.isoformat()}
    if isinstance(obj, PdfBlock):
        return _pdf_block_tag(obj)
    if isinstance(obj, TextBlock):
        return _text_block_tag(obj)
    if _is_entity(obj):
        return _entity_tag(obj)
    if isinstance(obj, (set, frozenset)):
        return {"__set__": sorted((to_serializable(v) for v in obj), key=repr)}
    if isinstance(obj, (list, tuple)):
        return [to_serializable(v) for v in obj]
    if isinstance(obj, dict):
        return {str(k): to_serializable(v) for k, v in obj.items()}
    raise SerializationError(f"Cannot serialize {type(obj).__qualname__}: {obj!r}")


def from_serializable(data: Any) -> Any:
    """L'inverso di `to_serializable`."""
    if data is None or isinstance(data, (bool, int, float, str)):
        return data
    if isinstance(data, list):
        return [from_serializable(v) for v in data]
    if isinstance(data, dict):
        if data.get("__promise__") is True and "id" in data:
            return _promise_from_tag(data)
        if "__enum__" in data:
            return _tag_to_enum(data["__enum__"])
        if "__date__" in data:
            return date.fromisoformat(data["__date__"])
        if "__set__" in data:
            return {from_serializable(v) for v in data["__set__"]}
        if "__pydantic__" in data:
            return _entity_from_tag(data["__pydantic__"])
        if data.get("__pdf_block__"):
            return _pdf_block_from_tag(data)
        if data.get("__text_block__"):
            return _text_block_from_tag(data)
        return {k: from_serializable(v) for k, v in data.items()}
    raise SerializationError(f"Cannot deserialize {type(data).__qualname__}: {data!r}")


def dumps(obj: Any, indent: Optional[int] = 2) -> str:
    return json.dumps(to_serializable(obj), indent=indent, ensure_ascii=False)


def loads(data: str) -> Any:
    return from_serializable(json.loads(data))


def dump(obj: Any, fp) -> None:
    json.dump(to_serializable(obj), fp, indent=2, ensure_ascii=False)


def load(fp) -> Any:
    return from_serializable(json.load(fp))
