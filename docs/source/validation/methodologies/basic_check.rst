..
    WARNING:
    
    This file describes the basic_check methodology for validating test outputs.
    Any changes to this file will change its SHA256 hash and invalidate all
    validation documents referencing this version.

=======================
Basic Check Methodology
=======================

**Purpose**
The basic_check methodology provides a lightweight validation that ensures program outputs are generated correctly and appear reasonable at first glance.

**Scope**
This methodology covers files in the test output directories (``tests/.../out/`` and ``tests/.../pages/``) and certifies that:

- The program successfully generates the expected output files
- The content appears reasonable and follows expected patterns
- A human has performed a basic visual inspection of the results

**Covered File Types**
- ``.csv`` files (e.g., ``investments.csv``, ``.log.csv``)
- ``.yaml`` files (e.g., ``investments_add_infos.yaml``)
- ``.pkl`` files (specifically ``results.pkl``)

**Protocol Steps**

1. **File Generation Check**: Verify that all expected output files are created by the program
2. **Basic Content Validation**: 
   - Check that CSV files contain data with expected columns
   - Verify YAML files have valid structure and contain expected keys
   - Ensure pickle files can be loaded and contain data
3. **Human Visual Inspection**: 
   - Quick review of output data for obvious anomalies
   - Confirmation that data appears reasonable for the test case
4. **Cross-reference Check**: Verify that ``results.pkl`` is derived from ``pdf_blks`` and ``txt_blks`` inputs

**Trust Level**
This methodology provides **weak certification** - it indicates the program functions correctly on test data and produces outputs that appear reasonable, but does not guarantee complete accuracy or comprehensive data validation.

**Applicable Context**
Use this methodology for routine testing and development verification where comprehensive manual review is not required.

Supported paths
===============

Vouching for a file under this methodology means the protocol above was applied to *that file*.
What that means concretely depends on what the file is, so each path this methodology covers says
so. All patterns are relative to the root of the repository the granted file lives in.

``tests/formats/**/out/*.csv``
    The tabular reference output of a format's test suite, in a **formats repository** -- the files
    ``freeports-dev make-tests`` writes and ``freeports-dev test`` afterwards compares against.
    A basic check here means the run that produced them completed, the columns are the ones
    expected, and a person has read the values looking for obvious nonsense. It includes
    ``.log.csv``, whose basic check is that its contents were read rather than merely produced.

``tests/formats/**/out/*.yaml``
    The non-tabular output of the same run, such as ``investments_add_infos.yaml``. A basic check
    here means the file parses, carries the keys expected of it, and its values were looked at.

``tests/formats/**/pages/**``
    The per-page fixtures of the same suite: the blocks the extraction saw and the result it
    reached for one page. A basic check here means the page was classified as the fixture says, and
    that a person confirmed it by eye against the report it came from.

.. note::

   These paths mean what they say **in a formats repository** -- one with a ``metadata/formats.csv``
   at its root, whose ``tests/formats/`` holds reports and the fixtures made from them. A directory
   of the same name in the extraction engine's own repository holds fixtures of the library rather
   than of any format, and this methodology does not describe what vouching for one of those would
   mean.

   Note also that the variant level between a format and its outputs is optional: both
   ``tests/formats/ASTERIA-EN23/out/funds.csv`` and ``tests/formats/ARCA-IT24/1/out/funds.csv`` are
   ordinary, which is why the patterns above are written with ``**`` rather than with ``*``.
