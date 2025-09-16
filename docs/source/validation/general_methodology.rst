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
of protocol in order to grant their trust