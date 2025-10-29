=========================
How the data is validated
=========================


We think that an important part of developing an istrument that parse human
input and aims to extract homogeneous information from an etherogeneus set of pdfs,
should be granting a the best of his contextual possibilities the correctness and
accuracy of the output.

We identify two opposite approaches to address this need:

* don't take any responsibility and force the user to accept an **EULA** that imply his duty to check any output
* slow down development and limit the scope of the project, outputting data that is already present on
  third party databases and taking the implicit responsibility of a inexistent or delayed work

we decided to place ourself in near the second end, trying to don't slow too much the work flow but to provide
some kind of grant to the end user.
For this reason we are committed to research and actively resonate in order to structure some
protocols that make us sure to share responsibility with the final user for the data that we output.
The user takes the duty of being conscious of the protocols used and should be aware and realistic
about the limitations of any tool used for extracting data. We take the duty to continue to be
transparent about the protocols used and to try to develop them at our best (and to grant their use).

We know that understanding and developing an opinion on the methodolgies used for validation is a work
by itself and for this reason we will try to be available for explanations and receptive to the critiques.

Our aim is to develop a system that try to be democratic in practice, so taking into account the different
backgrounds and expertises of the users and of the community.

We take responsibility for our mistakes because denying them would be dening being humans.
At the same time we know how it is difficult to admit error especially in the context we live in.
For this reason it is in our best interest to make reasonable evaluations for the health
of the project and for delivering reliable data.

********************
The general approach
********************

:doc:`This page <general_methodology>` of the documentation explains the general protocol used for granting some 
degree of reliability on the tests and in particular how we provide accountability for mistakes.
The aim is to provide the user a transparent idea of the validation pipeline so that he can evaluate the limits of it.

The content of the page is condensed in a hexadecimal encoded identifier calculated
applying ``SHA256`` hashing function to content of the ``.rst`` file used for generating it.
This file is compiled in the :doc:`HTML page <general_methodology>` that you can read on the documentation website.
The source file is in the repository at `docs/source/validation/general_methodology.rst <https://github.com/freeports>`_. It
contains the guide and explanation for how we performed the tests and how the final user can
track and reconstruct all the validation pipeline. In various files this file will be referenced
through his *hash* or with the beginning part of it (enough to distinguish it from other *hashes*) 

.. note::

    The hash ``8b1ba204bb69a0ade2bfcf65ef294a920f6bb361b317dba43c7ef29d96332b9b`` can be 
    shortened to ``8b1ba204`` if is enough to identify the full hash

.. tip::

    The ``SHA256`` of a file can be calculated on linux with the command ``sha256sum <filename>`` with
    ``<filename>`` the path of the file to hash

.. note::

    **What The FHash?!?** The hash function is a matematical function that take as input some data of arbitrary size
    and output a number (in this case in hexadecimal format) with a fixed size. These functions are used to map
    the content of a file in a code easy to verify and parse. The deterministic nature of the hash function grants
    that from the same input is generated the same *hash*. Opposingly the length of the output and the procedure
    for calculating it, grants that different files generate different *hashes* (this last adfermation is not analytically
    true but it is a reasonable approximantion under the assumption of the absence of `hash collisions <https://en.wikipedia.org/wiki/Hash_collision>`_ 
    and it is one of the reason on why hashing functions are widely used in computer science)


.. toctree::
   :maxdepth: 2
   :caption: Contents:

   general_methodology
   methodologies/basic_check
   methodoloogis/golden_standard
   methodologies/agreement_and_good_faith
   assertions/validation_algorithm_trustworthiness
