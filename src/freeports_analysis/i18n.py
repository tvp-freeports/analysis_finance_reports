import locale
import tempfile
from pathlib import Path
from importlib_resources import files
import gettext

loc = locale.getlocale()[0]
if loc is None:
    loc = "en_US.UFT-8"
lang = loc.split("_")[0]
translation = None
with tempfile.TemporaryDirectory() as tmp_dir:
    for f in (files("freeports_analysis.locales") / lang / "LC_MESSAGES").iterdir():
        translation_dir = Path(tmp_dir) / lang / "LC_MESSAGES"
        translation_dir.mkdir(parents=True, exist_ok=True)
        tmp_file = translation_dir / f.name
        tmp_file.write_bytes(f.read_bytes())
    translation = gettext.translation("messages", tmp_dir, [lang])
    translation.install()
_ = translation.gettext
