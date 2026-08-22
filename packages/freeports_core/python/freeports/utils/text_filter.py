"""Utilities for `text_filter` segment"""

from freeports import _native
from freeports._internals.core.match import MatchFund

normalize_string = _native.core.normalize_string
investment_fund_filter_data = _native.core.investment_fund_filter_data
extract_currency_from_text = _native.core.extract_currency_from_text
