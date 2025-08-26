import logging
import importlib

logger = logging.getLogger(__name__)


def _get_segment(segment_name, pipeline_modules):
    segment = {}
    for pipeline, module in pipeline_modules.items():
        try:
            segment[pipeline] = [getattr(module, segment_name)]
        except:
            pass


def get_pipes(format_name):
    module_name = format_name.lower().replace("-", "_").replace(".", "_")
    modules = {}
    try:
        module = importlib.import_module(
            f"freeports_analysis.formats.{module_name}", package=__package__
        )
        named_pipelines = []
        try:
            named_pipelines = module.pipelines
        except AttributeError:
            pass
        modules = {pipe.__name__: pipe for pipe in named_pipelines}
        modules |= {"": module}
    except ImportError:
        return {}
