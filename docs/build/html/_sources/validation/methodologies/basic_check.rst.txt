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