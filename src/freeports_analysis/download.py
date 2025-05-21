import requests as rq
import logging as log
from io import BytesIO
import os
logger=log.getLogger(__name__)
def download_pdf(url: str, pdf: str, save_pdf: bool):
    try:
        response=rq.get(url+pdf)
        response.raise_for_status()
    except Exception as e:
        logger.critical(e)
        raise e
    finally:
        file=BytesIO(response.content)
        if save_pdf:
            with open(pdf,'wb') as f:
                f.write(file.read())
            logger.debug("File %s saved on disk [in %s]",pdf,os.getcwd())
        return file
