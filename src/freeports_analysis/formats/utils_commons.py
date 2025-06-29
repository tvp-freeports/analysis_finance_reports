from typing import Any, Callable, TypeVar, ParamSpec


def normalize_string(string: str):
    string = string.strip()
    string = string.lower()
    string = " ".join(string.split())
    return string


def normalize_word(word: str):
    word = word.strip()
    word = "".join(word.split())
    return word


P = ParamSpec("P")
R = TypeVar("R")


def default_if_not_implemented(default_func: Callable[P, R]) -> Callable[P, R]:
    """Replace the decorated function with a default given as argument of the decorator
    if the decorated function raise a `NotImplementedError` or return `None`

    Parameters
    ----------
    default_func : Callable[P,R]
        default function

    Returns
    -------
    Callable[P,R]
        the default function if the deorated function is overwritten, if not the decorated function
    """

    def wrapper(primary_func):
        def func(*args, **kwargs):
            try:
                result = primary_func(*args, **kwargs)
                if result is not None:
                    return result
            except NotImplementedError:
                pass
            return default_func(*args, **kwargs)

        return func

    return wrapper


def overwrite_if_implemented(primary_func: Callable[P, R]) -> Callable[P, R]:
    """Replace the decorated default function with a function given as argument of the decorator
    if that function do not raise a `NotImplementedError` or return `None`

    Parameters
    ----------
    primary_func : Callable[P,R]
        implementation of a function

    Returns
    -------
    Callable[P,R]
        the function given as argument of the decorator if implemented,
        otherwise the decorated default
    """

    def wrapper(default_func):
        def func(*args, **kwargs):
            try:
                result = primary_func(*args, **kwargs)
                if result is not None:
                    return result
            except NotImplementedError:
                pass
            return default_func(*args, **kwargs)

        return func

    return wrapper
