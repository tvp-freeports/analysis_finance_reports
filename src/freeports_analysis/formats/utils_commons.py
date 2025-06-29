from typing import Callable, Any


def normalize_string(string: str):
    string = string.strip()
    string = string.lower()
    string = " ".join(string.split())
    return string


def normalize_word(word: str):
    word = word.strip()
    word = "".join(word.split())
    return word


def default_if_not_implemented(default_func):
    """Replace the decorated function with a default given as argument of the decorator
    if the decorated function raise a `NotImplementedError` or return `None`

    Parameters
    ----------
    default_func : Callable[Any,Any]
        default function

    Returns
    -------
    Callable[Any,Any]
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


def overwrite_if_implemented(primary_func):
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
