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