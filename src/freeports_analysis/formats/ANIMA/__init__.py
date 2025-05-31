from enum import Enum, auto
from lxml import etree
from typing import List, Tuple
import re
from rapidfuzz import fuzz
from .. import PDF_Block, Text_Block
import logging as log

logger = log.getLogger(__name__)



def pdf_filter(xml_root: etree.Element) -> List[PDF_Block]:
    parts = []
    hodings_header=xml_root.find(".//line[@text='Holdings']/font[@name='Helvetica-Bold']")
    if hodings_header is not None:
        rows=xml_root.xpath(".//line[font[@name='Helvetica-Light']]")
        parts=[ PDF_Block(PDF_BlockType.REGULAR_LINES_HOLDING_PAGES , {}, blk) for blk in rows ]
    return parts


def text_extract(pdf_blocks: List[PDF_Block], targets: List[str]) -> List[Text_Block]:
    text_part_list = []
    for i,row_block in enumerate(pdf_blocks):
        row = row_block.content
        row = row.lower()
        row = ' '.join(row.split())
        for target in targets:
            target_n = target.lower()
            target_n = ' '.join(target_n.split())
            if target_n != '' and target_n in row:
                text_part_list.append(
                    Text_Block(
                        Text_BlockType.LINE_TABLE_HOLDINGS,
                        {
                            "holdings": pdf_blocks[i-1].content,
                            "match": target,
                            "fair value": pdf_blocks[i+1].content,
                            "% net asset": pdf_blocks[i+2].content 
                        },
                        row_block
                    )
                )
    return text_part_list


def tabularize(text_block: Text_Block) -> dict:

    holdings_regex=  r'^\d{1,3}(?:,\d{3})*$'
    net_assets_regex=  r'^\d{1,2}(?:\.\d+)?$'
    fair_value_regex=  r'^\d{1,3}(?:,\d{3})*$'

    holdings_match = re.match(
        holdings_regex,
        ''.join(text_block.metadata["holdings"].split())
        )
    net_assets_match = re.match(
        net_assets_regex,
        ''.join(text_block.metadata["% net asset"].split())
        )
    fair_value_match = re.match(
        fair_value_regex,
        ''.join(text_block.metadata["fair value"].split())
    )
    parsed_row = None
    if holdings_match and net_assets_match and fair_value_match:
        parsed_row = {
            "holdings": int(holdings_match[0].replace(",","")),
            "company": text_block.metadata["match"],
            "fair value": int(fair_value_match[0].replace(",","")),
            "% net asset": float(net_assets_match[0])
        }
    else:
        logger.warning("Row not recognized: %s %s",text_block.metadata,text_block.content)
        parsed_row = {}
    return parsed_row


class PDF_BlockType(Enum):
    REGULAR_LINES_HOLDING_PAGES = auto()



class Text_BlockType(Enum):
    LINE_TABLE_HOLDINGS = auto()
