from freeports_analysis.formats.algorithms.commons import Pipeline
import investments as i

pipelines = {"investments": Pipeline(pdf_extract=i.pdf_extract)}

page_classify = Pipeline()
