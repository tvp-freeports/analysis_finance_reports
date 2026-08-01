import cProfile
import conf
import random
from pathlib import Path
import datetime
import freeports_analysis.main as fra

TEST_FORMATS_DIR = Path("../tests/formats/algorithms")
PROFILES_DIR = Path("profiles")

SHORT_DOCS_NAME = "S"
LONG_DOCS_NAME = "L"
TIMESTAMP = datetime.datetime.now().strftime("%y-%m-%d_%H,%M,%S")

n_long = 1
n_short = 1

l_docs = random.sample(conf.long_documents, n_long)
s_docs = random.sample(conf.short_documents, n_short)

print("Long documents used for profiling:")
for d in l_docs:
    print(f"\t{d}")
print("Short documents used for profiling:")
_s_docs = [s for s in s_docs]
if len(s_docs) % 2 == 1:
    _s_docs.append("")
for d1, d2 in zip(_s_docs[::2], _s_docs[1::2]):
    print(f"\t{d1}, {d2}")

config = conf.freeports_conf


pr = cProfile.Profile()
print("-----------------------------------")
pr.enable()
for i, s in enumerate(s_docs):
    print(f"Starting short document {i + 1} out of {n_short}")
    config["OUT_PATH"] = conf.OUT_PATH / s
    config["FORMAT"] = s
    config["PDF"] = TEST_FORMATS_DIR / s / "report.pdf"
    fra.main(config)

pr.disable()
pr.dump_stats(PROFILES_DIR / f"{SHORT_DOCS_NAME}{n_short}@{TIMESTAMP}.prof")
print("-----------------------------------")
pr.enable()
for i, l in enumerate(l_docs):
    print(f"Starting long document {i + 1} out of {n_long}")
    config["OUT_PATH"] = conf.OUT_PATH / l
    config["FORMAT"] = l
    config["PDF"] = TEST_FORMATS_DIR / l / "report.pdf"
    fra.main(config)
pr.disable()
pr.dump_stats(PROFILES_DIR / f"{LONG_DOCS_NAME}{n_long}@{TIMESTAMP}.prof")
print("-----------------------------------")
