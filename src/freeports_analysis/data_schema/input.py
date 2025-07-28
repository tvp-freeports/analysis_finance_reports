from sqlalchemy.orm import DeclarativeBase
from abc import ABC


class InputDB(DeclarativeBase):
    pass


class Company(ABC):
    pass


class List(InputDB):
    pass


class Ticker(InputDB):
    pass


class Market(InputDB):
    pass
