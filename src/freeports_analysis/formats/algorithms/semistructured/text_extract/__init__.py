"""Text extraction algorithms for semi-structured document processing.

This module provides text extraction functions specifically designed for
semi-structured documents.
"""

from typing import Optional
from pydantic import BaseModel, model_validator
from freeports_analysis.formats.utils.text_extract import standard_text_extraction

# class InputStandard(BaseModel):
#     market_value: int
#     nominal_quantity: Optional[int] = None
#     perc_net_assets: Optional[int] = None
#     acquisition_cost: Optional[int] = None
#     acquisition_currency: Optional[int] = None
#     geometrical_indexes: Optional[bool] = True
#     merge_prev: Optional[bool] = True

#     @model_validator(mode='after')
#     def validate_unique_non_zero(self):
#         values = [
#             self.market_value,
#             self.nominal_quantity,
#             self.perc_net_assets,
#             self.acquisition_cost,
#             self.acquisition_currency
#         ]  # Raccogli tutti i valori
#         values = [v for v in values if v is not None]
#         # 1. Controlla zeri
#         if 0 in values:
#             raise ValueError("All values must be non-zero")

#         # 2. Controlla duplicati
#         if len(values) != len(set(values)):
#             raise ValueError("All values must be unique")

#         return self


# def standard(arg: InputStandard):
#     return standard_text_extraction(
#         market_value_pos=arg.market_value,
#         nominal_quantity_pos=arg.nominal_quantity,
#         perc_net_assets_pos=arg.perc_net_assets,
#         acquisition_currency_pos=arg.acquisition_currency,
#         acquisition_cost_pos=arg.acquisition_cost,
#         geometrical_indexes=arg.geometrical_indexes,
#         merge_prev=arg.merge_prev
#     )
