"""Internationalization module for locale detection and message translation.

This module handles locale detection and provides translation functionality
for the application. It supports both POSIX and Windows systems and falls
back to English if no suitable locale is found.

Attributes
----------
LOC : Optional[str]
    Detected locale identifier
lang : str
    Language code extracted from locale
TRANSLATION : Optional[gettext.NullTranslations]
    Translation object for the detected locale
_ : Callable[[str], str]
    Translation function for message strings
"""

import os
import locale
import tempfile
from pathlib import Path
import gettext
from importlib_resources import files

LOC = None
if os.name == "posix":
    LOC = locale.getlocale()[0]
elif os.name == "nt":
    LOC = locale.getlocale()[0][:2].lower()

if LOC is None:
    LOC = "en_US.UFT-8"
lang = LOC.split("_")[0]
TRANSLATION = None
with tempfile.TemporaryDirectory() as tmp_dir:
    for f in (files("freeports_analysis.locales") / lang / "LC_MESSAGES").iterdir():
        translation_dir = Path(tmp_dir) / lang / "LC_MESSAGES"
        translation_dir.mkdir(parents=True, exist_ok=True)
        tmp_file = translation_dir / f.name
        tmp_file.write_bytes(f.read_bytes())
    TRANSLATION = gettext.translation("messages", tmp_dir, [lang])
    TRANSLATION.install()
_ = TRANSLATION.gettext
