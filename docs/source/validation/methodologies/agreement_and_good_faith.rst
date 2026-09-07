..
    WARNING:
    
    This file describes the agreement_and_good_faith methodology for validating
    assertions through personal agreement and technical understanding.
    Any changes to this file will change its SHA256 hash and invalidate all
    validation documents referencing this version.

====================================
Agreement and Good Faith Methodology
====================================

**Purpose**
The agreement_and_good_faith methodology certifies that a contributor has read, understood, and agrees with the content of an assertion based on their technical capabilities and in good faith.

**Scope**
This methodology covers assertion files in the ``docs/source/validation/assertions/`` directory, including:

- ``.rst`` files (documentation assertions)
- ``.md`` files (markdown assertions) 
- ``.png`` files (visual assertions)
- ``.svg`` files (diagram assertions)

**Protocol Steps**

1. **Thorough Reading/Review**:
   - Completely read the assertion content (text, diagrams, or visual elements)
   - Ensure understanding of all technical concepts and claims presented
2. **Technical Capability Assessment**:
   - Evaluate the assertion within the scope of your technical expertise
   - Identify any areas beyond your current understanding
   - Seek clarification if any aspect is unclear
3. **Good Faith Agreement**:
   - Confirm agreement with the assertion content to the best of your understanding
   - Acknowledge any limitations in your technical knowledge
   - Document any reservations or qualifications if applicable
4. **Responsibility Acceptance**:
   - Accept responsibility for the consequences of agreeing with the assertion
   - Commit to updating your position if new information emerges

**Trust Level**
This methodology provides **personal certification** - it represents an individual's good-faith agreement based on their current technical understanding, but does not guarantee objective truth or comprehensive verification.

**Applicable Context**
Use this methodology for:
- Assertions about software behavior or capabilities
- Documentation accuracy claims
- Architectural or design principle assertions
- Best practice recommendations
- Compliance or standard adherence statements

**Limitations**
- Agreement is subjective and based on individual technical capabilities
- Does not replace empirical testing or formal verification
- Should be used alongside other validation methodologies for critical assertions

**Ethical Considerations**
Contributors should only use this methodology when they have genuinely:
- Made reasonable efforts to understand the assertion
- Applied their technical knowledge appropriately
- Acted in good faith without conflicts of interest

Supported paths
===============

What this methodology covers is not a file a program produced. It is an **assertion**: a statement
about the software, published as prose, that a person can read, understand and agree with. So its
declared path names documentation rather than test output, and a grant under it is a claim about a
*statement*, not about any bytes an extraction wrote.

``docs/source/validation/assertions/**``
    An assertion published in the documentation of **the repository the granted file lives in** --
    for the freeports engine, ``analysis_finance_reports``. Granting one means the protocol above
    was applied to that text: it was read completely, judged within the granter's own technical
    competence, and agreed with in good faith. It covers whatever form the assertion takes there,
    prose or diagram alike.

.. note::

   Another project adopting this methodology would keep its assertions somewhere of its own, and
   this pattern would not reach them. That is deliberate rather than an oversight: a path means
   something only inside a repository, and the honest way to extend this methodology to a different
   layout is to say so in a page of your own rather than to widen this pattern until it means
   nothing.

   This is also why the paths here look nothing like those of ``basic check`` and
   ``golden standard``. Those methodologies vouch for what a program produced; this one vouches for
   what a person wrote and another person read.
