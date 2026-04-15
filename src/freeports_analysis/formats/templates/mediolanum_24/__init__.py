from . import inv_managers


def compute_page_class(classification):
    inv_managers = False
    for i, val in enumerate(classification):
        if inv_managers and val is None:
            classification[i] = "inv_managers"
        elif val == "inv_managers_begin":
            inv_managers = True
        elif val == "inv_managers_end":
            inv_managers = False
    return classification
