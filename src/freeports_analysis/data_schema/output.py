from sqlalchemy.orm import DeclarativeBase
from abc import ABC


class OutputDB(DeclarativeBase):
    pass


class FinancialData(ABC):
    pass


class Equity(OutputDB):
    pass


class Bond(OutputDB):
    pass


class Subfund(OutputDB):
    pass
