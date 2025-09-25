import logging
import importlib

logger = logging.getLogger(__name__)


def _get_segment(segment_name, pipeline_modules):
    segment = {}
    for pipeline, module in pipeline_modules.items():
        try:
            funcs = getattr(module, segment_name)
            segment[pipeline] = funcs if isinstance(funcs, list) else [funcs]
        except AttributeError:
            pass
    return segment


def get_pipes(format_name):
    module_name = (
        format_name.lower().replace("-", "_").replace(".", "_").replace("/", "_")
    )
    modules = {}
    try:
        module = importlib.import_module(
            f"{__name__}.{module_name}",
            package=__package__,
        )
        named_pipelines = []
        try:
            named_pipelines = module.pipelines
        except AttributeError:
            pass
        modules = {pipe.__name__: pipe for pipe in named_pipelines}
        modules |= {"": module}
        pdf_filter_segment = _get_segment("pdf_filter", modules)
        text_extract_segment = _get_segment("text_extract", modules)
        deserialize_segment = _get_segment("deserialize", modules)
        return pdf_filter_segment, text_extract_segment, deserialize_segment
    except ModuleNotFoundError:
        return {}, {}, {}
