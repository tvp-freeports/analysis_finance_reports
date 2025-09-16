..
    WARNING:
    
    This file is the main file that describe the methodology for validate
    and check the validation of the tests. This file is identified by his
    hash so every change to it will result of the invalitation of all the
    parts referring to his hash, be careful with modifying this file, a 
    incorrect update of this file can result in the impossibility for the
    user to trust the output of the software and for the developers to
    grant the correct functioning of it. So before modifying is appropriate
    to understand the validation mechanism.


===================
General methodology
===================

In the repository there is one directory dedicated to the tests, this directory is called ``tests``.
Another directory is dedicated to the accountability and is the one that contain information on the
protocols used to grant the functioning of the software and it is called ``validation```.

We develop different tests for granting the accuracy of our program, but some of them require some kind
of protocol in order to grant their trust.

There are two different file types that we grant the content of:

1. **test results**:
   in the ``tests`` directory, files of type

   * ``.csv``
   * ``.yaml``
   * ``.pkl``
   * ``.png``
   * ``.pdf``

2. **assertions**
   in the ``docs/source/validation/assertions``, directory files of type

   * ``.rst``
   * ``.md``
   * ``.png``
   * ``.svg``
   
the files are granted through a certain specific ``methodology`` that imply a protocol.
The different methodologies used are documented in the ``docs/source/validation/methodologies`` directory
through ``.rst`` files.

********************
Validation documents
********************

In the `validation <https://github.com>`_ directory are present some bash scripts used for help the user
to check for accountability and a directory called `validation/documents <https://github.com>`_.
Each ``.yaml`` file in this directory is a document used for grant the use of a certain protocol
in some specific context. In particular each file is refears to one developer or contributor
and it has a certain structure:

.. code-block:: yaml

    version: <hash_general_methodology>
    who:
      name: <complete_name>
      email: <email>
      pubkey-id: <id_public_key>
    methodologies:
      - name: <name_methodology>
        sha256: <methodology_hash>
          .
          .
          .
      - name: <name_methodology>
        sha256: <methodology_hash>
      .
      .
      .
    data:
      - methodology: <name_methodology>
        files:
          - path: <path_to_the file>
            sha256: <file_hash>
          - path: <path_to_the file>
            sha256: <file_hash>
          .
          .
          .
      - methodology: <name_methodology>
        files:
          - path: <path_to_the_file>
            sha256: <file_hash>
        .
        .
        .
    sig: <crittographic signature of the document>

The first section is the ``who`` section that contains information about the contributor
that is accountable or responable of having followed a certain protocol. This section is composed by

* ``name``: is the complete name of who own the document and who is responable for its content;
  in particular is the owner of the crittographic keys used to sign the file
* ``email``: is the email of ``<complete_name>``, the user can be notified of incongruences through
  that channel and it is the link to his physical person
  (in particular the keys are stored on the `OpenPGP key server <https://keys.openpgp.org/>`_
  that require email verification to search pub-keys by email)
* ``pubkey-id``: is the unique identifier of the key pair used to sign the document

The section ``methodologies`` contains the list of all specific methodologies used to grant some
level of trust in some files, each entry has

* ``name``: should be named as the corresponding documentation file, replacing the character ``_`` to a space, 
  lower casing and removing the file exstension ``.rst`` 
  (for example the file ``docs/source/validation/methodologies/basic_check.rst`` will be identified with ``basic check``).
* ``sha256``: identify the precise content that the entry refears to; if the protocol for a methodology get updated the hash will
  change and all the document referring to that version of the methodology get invalidated consequently.

The last section called ``data`` and is composed by a list of files cocovered by some kind of grant.
Each entry is composed with sections dedicated to certify the application of a methodology to a list of files.
The name of the methodology has to be the same that compose one of the different values ``name`` in the ``mathodologies`` section.
For each methodologies are associated some files composed by a ``path`` that should be relative to the ``docs/source/validation/assertions``
directory or the ``tests`` directory depending if the covered file is a **test result** or an **assertion**.

.. danger::
  Naming a test result or an assertion in the same manner would conduct to ambiguity so it is not considered valid,
  the user that write the methodology should be responable for checking that this possibility never happens.
  If it will happens the hope is that from the methodology used is clear the class of the file cocovered.

These section are the one che compose the content of the inner part of the document. In addition to that there is a last entry
that is the ``sig`` entry. This entry is generated using the private part of the key pair identified in the ``who`` section
to sign the output of the remaining part of the document (the other sections). In particular the signed version is
the ``.yaml`` file stripped from meaningless white spaces and with the mapping entry reordered in alphabetic order.

.. tip::

  You can get the precise version that is signed (normalized and without signature) launching on linux:
  
  .. code:: console

    yq -y -S 'del(.sig)' <yaml-document-path>


Specifically two commands ``who-grants`` and ``granted-by`` that are copmelementary and respectively
they take in input 
