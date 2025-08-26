from typing import Optional
from pydantic import BaseModel
from freeports_analysis.formats_utils.deserialize import standard_deserialization

# class InputStandard(BaseModel):
#     cost_and_value_interpret_int: Optional[bool] = True
#     quantity_interpret_float: Optional[bool] = False


# def standard(arg: InputStandard):
#     return standard_deserialization(
#         cost_and_value_interpret_int=arg.cost_and_value_interpret_int,
#         quantity_interpret_float=arg.quantity_interpret_float
#     )
