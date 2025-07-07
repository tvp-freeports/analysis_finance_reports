=================
Command reference
=================

The command can be used launching 

.. code-block:: console

    freeports

from the command line. To get a contextual help use the option ``--help`` shortened to ``-h``

.. code-block:: console

    freeports -h

the options can be specified in different ways that overwrite each other. The same option can be specified in 3 different ways:

1. **configuration file**
2. **environment variables**
3. **command line options**

option not specified default to values specified in the :ref:`conf_parse submodule <freeports\_analysis.conf\_parse>`. 
Those are overwritten by the options specified in the **configuration file**, then the result is overwritten by the **environment variables**,
**command line options** and finally if when in :ref:`BATCH MODE <batch_mode>` by the *job contextual options*.
The option available to be overwritten and how are documented in the respective reference page:

.. toctree::
   :maxdepth: 1
   :caption: Reference config pages:

   config/cmd_args.rst
   config/env_variables.rst
   config/config_file.rst 

After specified overwritten the options are overwritten as described in :ref:`the section about validation <conf_validation>`.
Each method of overwriting also has a specific validation mechanism documented in the respective page and applied before
the validation of resulting configuration.
Each way of specify option set one of the program option described in this page.
The options here documented have an effecton the behaviour of the ``freeports`` call.

-----------
The options
-----------


+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| Option            | Type                    | Description                                              | Default                |
+===================+=========================+==========================================================+========================+
| ``VERBOSITY``     | ``int``                 | Describe how much the program verbosity                  | 2                      |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``BATCH``         | ``Path``                | If set to path of batch file, it triggers ``BATCH MODE`` |                        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``N_WORKERS``     | ``int``                 | Number of parallel processes in ``BATCH MODE``           |                        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``OUT_CSV``       | ``Path``                | File where to output ``csv`` files                       | ``/dev/stdout``        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``SAVE_PDF``      | ``bool``                | If set and ``URL`` is specified, it save the input pdf   | ``True``               |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``URL``           | ``str``                 | Url of the pdf to take in input                          |                        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``PDF``           | ``Path``                | Path to local pdf                                        |                        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``FORMAT``        | :py:class:`PdfFormats`  | Format to parse the pdf document                         |                        |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+
| ``CONFIG_FILE``   | ``Path``                | Custom config file location                              | Calculated dynamically |
+-------------------+-------------------------+----------------------------------------------------------+------------------------+

"""""""""""""
``VERBOSITY``
"""""""""""""

This values goes from 0 to 5, 0 indicate min verbosity called ``CRITICAL VERBOSITY`` and 4 indicate the max verbosity also called ``DEBUG VERBOSITY``, 5 is the ``NOSET VERBOSITY``.
The meaning of the others levels are the ones used by the python `logging package <https://docs.python.org/3/library/logging.html>`_:

+-----------+----------------------------------------------------------------------------+
| freeports | `logging <https://docs.python.org/3/library/logging.html#logging-levels>`_ |
+===========+============================================================================+
| 0         | ``loggign.CRITICAL``                                                       |
+-----------+----------------------------------------------------------------------------+
| 1         | ``logging.ERROR``                                                          |
+-----------+----------------------------------------------------------------------------+
| 2         | ``loggign.WARNING``                                                        |
+-----------+----------------------------------------------------------------------------+
| 3         | ``loggign.INFO``                                                           |
+-----------+----------------------------------------------------------------------------+
| 4         | ``logging.DEBUG``                                                          |
+-----------+----------------------------------------------------------------------------+
| 5         | ``logging.NOSET``                                                          |
+-----------+----------------------------------------------------------------------------+

"""""""""""""""""""""""""""""""""
``URL``, ``PDF`` and ``SAVE_PDF``
"""""""""""""""""""""""""""""""""

When not in ``BATCH MODE`` one between ``URL`` or ``PDF`` has to be specified.
If ``URL`` is specified the program use the pdf file corresponding to the url,
if ``PDF`` is specified it load a pdf file from local filesystem and if both are specified
it tries to load from local storage, then fallback to the url.
If both are specified and ``SAVE_PDF`` is ``True``, if the file is not present locally, it download it
and save on disk with name indicate by ``PDF`` option.

"""""""""""
``OUT_CSV``
"""""""""""

When not in ``BATCH MODE`` it indicate the file where to output the resulting ``csv`` parsed from the pdf document.

.. note::
    
    The ``OUT_CSV`` default on ``Windows`` systems is ``CON``


""""""""""
``FORMAT``
""""""""""

It indicate which algorithm to use to parse the pdf, these algorithms are called the 'formats' of the pdf reports.
It is mandatory to specify this variable if no ``URL`` is provided, if it is provided the format try to be inferred using
a mapping file that map different url regular expressions to a format. The file is called ``format_url_mapping.yaml`` in the source code.

"""""""""""""""
``CONFIG_FILE``
"""""""""""""""

This option indicate the config file loaded to overwrite the default options, this option can only be specified
using an environment variable or using a command line argument, and it is evaluated before any other option.

.. _conf_validation:

-------------------------------------
Validation of resulting configuration
-------------------------------------

Each way of specify options have his algorithm to validate the user choice, but after those checks 
it is performed a consistency check on the resulting configuration.
Noticebly the most important performed chekcs are:

* In ``BATCH MODE`` ``OUT_CSV`` is the name of an archive or of a directory
* After *job contextual options* overwriting at least one between ``PDF`` or ``URL`` is defined



.. _batch_mode:

--------------
``BATCH MODE``
--------------

This mode permit to process different files all at one in parallel. This mode is caratterized by the ``BATCH``
variable set to a *batch csv file*, ``OUT_CSV`` to a directory name or ``.tar.gz`` archive and 
optionally ``N_WORKERS`` to a number (if not set default to number of available CPUs). 
The *batch csv file* is a csv file with some header that indicate the option to overwrite to the 
resulting configuration. These option are called *job contextual options* and each row of the csv file is called a *job*.
The available overwrittables options are:

+----------------+--------------------+
| Header         | Overwritten option |
+================+====================+
| ``url``        | ``URL``            |
+----------------+--------------------+
| ``save pdf``   | ``SAVE_PDF``       |
+----------------+--------------------+
| ``pdf``        | ``PDF``            |
+----------------+--------------------+
| ``format``     | ``FORMAT``         |
+----------------+--------------------+
| ``prefix out`` | *See below*        |
+----------------+--------------------+

the header is case insensitive, so for example *url*, *URL* and *Url* are considered the same header.
the bool matching is done so that cast to ``True`` if csv value is one between (case insensitive) 
*true, on, yes, y, t, 1* to ``False`` if between *false, off, no, n, f, 0*.

""""""""""""""""""""""""""""""
``OUT_CSV`` and ``prefix out``
""""""""""""""""""""""""""""""

When in ``BATCH MODE``, ``OUT_CSV`` has to be a directory or a ``.tar.gz`` archive. 
The ``prefix out`` cell add to the resulting configuration a ``PREFIX_OUT_CSV`` option.
The program create if it doesn't exists a directory named as ``OUT_CSV`` if is not an archive
or the name of the archive without ``.tar.gz`` exstension and for each *job*, save an output file
named ``{PREFIX_OUT_CSV}-{FORMAT}.csv``. Than if was specified ``OUT_CSV`` as an archive, the directory
is compressed into ``.tar.gz``. If the directory didn't existed and an archive is created, after creation
the directory is deleted from the filesystem.
