"""All instruments to deal with structured formats.
A pipeline segment should be considered structured when the implementation of it
is common for many format and what it change is the value of some parameters
(the number of implementations is a fixed number and have many segments associated with them).

It is characterized by a definition of the algorithm associated with the segment fully located in the library
and the value of the parameters the only thing present in the format repo.
"""
