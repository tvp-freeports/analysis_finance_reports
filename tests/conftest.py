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
    "AMUNDI2": "https://www.amundi.it/dl/doc/annual-report/IT0005491680/ITA/ITA/20241230",
    "EURIZON": "https://www.eurizonam.hr/UserDocsImages//LUX/SAR_HR_en_LU1341630033_YES_2023-06-30.pdf",
    "ANIMA": "https://www.animasgr.it/d/EN/downloads/Documents/Anima%20Funds%20plc%2031%20December%202023%20Financials.pdf",
    "FIDEURAM": "https://www.fideuramassetmanagement.ie/upload/File/pdf/Policy_FAMI/WILLERFUNDS/DOC/FIDEURAM_WILLERFUNDS_Semi-Annual%2028.02.2023.pdf",
}

single_page_tests = [
    ("ANIMA", 544),
    ("EURIZON", 254),
    ("AMUNDI", 43),
    ("ASTERIA", 78),
    ("AMUNDI2", 558),
    ("FIDEURAM", 51),
    ("EURIZON_OLD", 97),
]

all_pages_tests = ["ANIMA", "EURIZON", "AMUNDI", "ASTERIA", "AMUNDI2", "FIDEURAM"]
