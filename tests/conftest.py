from pathlib import Path
import os
import shutil
import data

out_dir = Path(__file__).parent / "output/"
data_dir = Path(__file__).parent / "data/"

try:
    shutil.rmtree(out_dir)
except FileNotFoundError:
    pass
out_dir.mkdir()

url_example_formats = {
    "AMUNDI": "https://www.amundi.com/dl/doc/annual-report/LU1883342377/ENG/ITA/20240630?inline",
    "EURIZON": "https://www.eurizonam.hr/UserDocsImages//LUX/SAR_HR_en_LU1341630033_YES_2023-06-30.pdf",
    "ANIMA": "https://www.animasgr.it/d/EN/downloads/Documents/Anima%20Funds%20plc%2031%20December%202023%20Financials.pdf",
}

single_page_tests = [
    ("ANIMA", 544),
    ("EURIZON", 254),
    ("AMUNDI", 43),
    ("ASTERIA", 78),
    ("EURIZON_OLD", 97),
]

all_pages_tests = ["ANIMA", "EURIZON", "AMUNDI", "ASTERIA", "EURIZON_OLD"]
