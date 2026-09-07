..
    WARNING:
    
    This file describes the golden_standard methodology for comprehensive validation.
    Any changes to this file will change its SHA256 hash and invalidate all
    validation documents referencing this version.

===========================
Golden Standard Methodology
===========================

**Purpose**
The golden_standard methodology provides comprehensive validation through explicit, manual verification of all input data and output results.

**Scope**
This methodology extends basic_check with rigorous manual verification of:

- All ``pdf_blks`` and ``txt_blks`` input files
- Complete data extraction verification
- Explicit confirmation that no data is missing from results

**Covered File Types**
- All files covered by basic_check methodology
- Additional verification of input block files (``pdf_blks``, ``txt_blks``)

**Protocol Steps**

1. **Complete basic_check protocol** (all steps)
2. **Input Block Verification**:

   - Manually review every ``pdf_blk`` and ``txt_blk`` file
   - Verify that all relevant data from input blocks is captured in ``results.pkl``
   - Explicitly confirm no data is omitted or incorrectly parsed

3. **Log Analysis**:

   - Verify that ``.log.csv`` contains only true anomalies or is empty
   - Each warning in the log must be reviewed and confirmed as legitimate

4. **Comprehensive Data Validation**:

   - Cross-reference every data point in output files with source blocks
   - Verify data consistency across all output formats (CSV, YAML, pickle)

5. **Edge Case Verification**:

   - Check handling of unusual data formats or structures
   - Verify error handling and logging for problematic inputs

**Trust Level**
This methodology provides **strong certification** - it represents thorough manual verification that all data is correctly processed and no information is lost during extraction.

**Applicable Context**
Use this methodology for:
- Release candidate validation
- Critical data processing verification  
- Situations requiring high confidence in data accuracy
- Validation of core algorithm functionality

**Relationship to basic_check**
The golden_standard methodology includes and extends all basic_check requirements, providing a superset of validation guarantees.

Supported paths
===============

This methodology covers the same files as ``basic check`` -- it is that protocol plus the manual
verification described above, not a different subject -- so the patterns are the same, and what
changes is what a grant on them claims. All patterns are relative to the root of the repository the
granted file lives in.

``tests/formats/**/out/*.csv``
    The tabular reference output of a format's test suite in a **formats repository**. A golden
    standard grant here claims more than that the values look reasonable: every one of them was
    cross-referenced against the blocks it was extracted from, and ``.log.csv`` was read line by
    line and found to contain only anomalies confirmed as genuine.

``tests/formats/**/out/*.yaml``
    The non-tabular output of the same run, verified to the same standard.

``tests/formats/**/pages/**``
    The per-page fixtures: the blocks the extraction saw, and the result it reached. This is where
    the additional claim actually rests -- a golden standard grant says every block was reviewed and
    that nothing relevant present in the page is missing from the result.

.. note::

   As with ``basic check``, these paths describe a **formats repository**, and the variant level
   between a format and its outputs is optional -- hence ``**`` rather than ``*``.
