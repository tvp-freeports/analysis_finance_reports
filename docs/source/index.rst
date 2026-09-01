=========
freeports
=========

**Structured data out of financial reports published as PDF.**

Funds disclose what they hold, what they are worth and how they classify themselves — in annual
reports, in PDF, laid out however each issuer chose. ``freeports`` reads those documents and writes
tables you can compute on. The engine knows nothing about any particular report: support for a
layout lives in a separately maintained **formats repository**, which is what lets coverage grow
without touching the engine.

.. important::

   Before relying on the output, read :doc:`what the project does and does not claim
   <whitepaper/validation>`, and the :doc:`validation section <validation/index>` it summarises.

Start here
==========

The :doc:`whitepaper <whitepaper/index>` is the main document: it opens with the problem and the
project's position on being trusted with data — readable without a technical background — and then
covers installation, use, the execution model, writing a format, and the design decisions behind
all of it.

.. toctree::
   :maxdepth: 2
   :caption: The whitepaper

   whitepaper/index

.. toctree::
   :maxdepth: 2
   :caption: Reference

   API
   rustdoc

.. toctree::
   :maxdepth: 2
   :caption: Trust and provenance

   validation/index

.. toctree::
   :maxdepth: 2
   :caption: Contributing

   contribute
   dev/index

.. note::

   This project is under active development.
