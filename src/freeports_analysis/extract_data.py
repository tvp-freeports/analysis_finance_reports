from text_parts import Text_part
import logging as log
import re

logger = log.getLogger(__name__)


def EURIZON(text_bit: Text_part) -> dict:
    complete_pattern = re.match(
        r"(?P<nominal>[0-9.,]+)\s+(?P<company>.+?)\s*(?P<rate>\d[0-9.,]+%)\s*(?P<maturity>\d{2}/\d{2}/\d{4})\s+(?P<currency>[A-Z]{3})\s+(?P<acquisition>[0-9.,]+)\s+(?P<carrying>[0-9.,]+)\s+(?P<netassets>[0-9.,]+)",
        text_bit.content,
    )
    # Pattern senza interest rate e maturity
    reduced_pattern = re.match(
        r"(?P<nominal>[0-9.,]+)\s+(?P<company>.+?)\s+(?P<currency>[A-Z]{3})\s+(?P<acquisition>[0-9.,]+)\s+(?P<carrying>[0-9.,]+)\s+(?P<netassets>[0-9.,]+)",
        text_bit.content,
    )
    parsed_row = None
    if complete_pattern:
        parsed_row = {
            "Matched Company": text_bit.metadata["company_match"],
            "Nominal Value": complete_pattern.group("nominal"),
            "Company": complete_pattern.group("company"),
            "Interest Rate": complete_pattern.group("rate"),
            "Maturity": complete_pattern.group("maturity"),
            "Currency": complete_pattern.group("currency"),
            "Acquisition Cost": complete_pattern.group("acquisition"),
            "Carrying Value": complete_pattern.group("carrying"),
            "Net Assets": complete_pattern.group("netassets"),
        }
    elif reduced_pattern:
        parsed_row = {
            "Matched Company": text_bit.metadata["company_match"],
            "Nominal Value": reduced_pattern.group("nominal"),
            "Company": reduced_pattern.group("company"),
            "Interest Rate": None,
            "Maturity": None,
            "Currency": reduced_pattern.group("currency"),
            "Acquisition Cost": reduced_pattern.group("acquisition"),
            "Carrying Value": reduced_pattern.group("carrying"),
            "Net Assets": reduced_pattern.group("netassets"),
        }
    else:
        logger.warning("Row not recognized: %i", count)
    return parsed_row


def _DEFAULT_tabulizer():
    pass


def tabularize(text_bits: List[Text_part], tabularize_func) -> List[dict]:
    return [tabularize_func(x) for x in text_bits]
