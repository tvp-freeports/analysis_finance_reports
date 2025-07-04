from typing import Optional, Tuple, List
from lxml import etree


def is_contained(
    blk: etree.Element,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if a block's bounding box is fully contained within specified ranges.

    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    x_range : Optional[Tuple[float, float]], optional
        Horizontal range (min_x, max_x)
    y_range : Optional[Tuple[float, float]], optional
        Vertical range (min_y, max_y)

    Returns
    -------
    bool
        True if entire bbox is within ranges, False otherwise
    """
    bounds = get_bounds(blk)

    if x_range is not None:
        l = x_range[0]
        r = x_range[1]
        if l is not None and bounds[0][0] < l:
            return False
        if r is not None and bounds[0][1] > r:
            return False

    if y_range is not None:
        t = y_range[0]
        b = y_range[1]
        if t is not None and bounds[1][0] < t:
            return False
        if b is not None and bounds[1][1] > b:
            return False

    return True


def select_inside(blocks, x_range, y_range):
    return [blk for blk in blocks if is_contained(blk, x_range, y_range)]


def select_outside(blocks, x_range, y_range):
    return [blk for blk in blocks if not is_contained(blk, x_range, y_range)]


def get_bounds(blk: etree.Element) -> list | None:
    bbox = blk.xpath(".//@bbox")
    if not bbox:
        return None

    coords = [float(c) for c in bbox[0].split()]
    coords = ((coords[0], coords[2]), (coords[1], coords[3]))
    return coords


def get_position(blk: etree.Element, mean: bool) -> list | None:
    """Return the coordinates or center of a bounding box from a PDF block element.
    Parameters
    ----------
    blk : etree.Element
        XML element representing the PDF block, expected to contain a 'bbox' attribute.
    mean : bool
        If True, return the center point (x, y) of the bounding box.
        If False, return the full bounding box coordinates (x0, y0, x1, y1).
    Returns
    -------
    list of float or None
        A list of float values representing either the full bounding box or its center.
        Returns None if no 'bbox' attribute is found.
    """

    bbox = blk.xpath(".//@bbox")
    if not bbox:
        return None

    coords = [float(c) for c in bbox[0].split()]  # x0, y0, x1, y1

    if mean:
        x_center = (coords[0] + coords[2]) / 2
        y_center = (coords[1] + coords[3]) / 2
        coords = (x_center, y_center)
    else:
        coords = (
            ((coords[0], coords[1]), (coords[0], coords[3])),
            ((coords[2], coords[1]), (coords[2], coords[3])),
        )

    return coords


def get_size(blk: etree.Element) -> Tuple[float, float]:
    x_bounds, y_bounds = get_bounds(blk)
    return (x_bounds[1] - x_bounds[0], y_bounds[1] - y_bounds[0])


def get_table_positions(
    blocks: List[etree.Element],
    small_rule: Tuple[bool, bool] = (True, True),
    use_ruler_pos: Tuple[bool, bool] = (True, True),
) -> List[Tuple[int, int]]:
    """Compute row and column index for each block assuming tabular layout.

    Parameters
    ----------
    blocks : list of etree.Element
        List of XML elements representing blocks, each with a 'bbox' attribute.

    Returns
    -------
    list of tuple of int
        A list of (row, column) indices corresponding to each block.
    """
    # Calcola i centri dei blocchi
    small_rule_row, small_rule_col = small_rule
    use_ruler_pos_row, use_ruler_pos_col = use_ruler_pos

    choose_ruler_row = min if small_rule_row else max
    choose_ruler_col = min if small_rule_col else max

    column_indexes = [None for blk in blocks]
    rows_indexes = [None for blk in blocks]

    index_sizes = [(i, get_size(blk)) for i, blk in enumerate(blocks)]
    index_pos = [(i, get_position(blk, mean=True)) for i, blk in enumerate(blocks)]
    x_rulers_col = []
    y_rulers_row = []
    while None in column_indexes or None in rows_indexes:
        curr_column = len(x_rulers_col)
        curr_row = len(y_rulers_row)
        index_height_no_row = [
            (i, size[1]) for i, size in index_sizes if rows_indexes[i] is None
        ]
        index_wide_no_col = [
            (i, size[0]) for i, size in index_sizes if column_indexes[i] is None
        ]
        x_bounds_ruler_col = None
        x_pos_ruler_col = None
        if len(index_wide_no_col) > 0:
            index_size_ruler_col = choose_ruler_col(
                index_wide_no_col, key=lambda x: x[1]
            )
            ruler_col_index = index_size_ruler_col[0]

            x_bounds_ruler_col = get_bounds(blocks[ruler_col_index])[0]
            x_pos_ruler_col = index_pos[ruler_col_index][1][0]

            x_rulers_col.append((curr_column, x_pos_ruler_col))

        y_bounds_ruler_row = None
        if len(index_height_no_row) > 0:
            index_size_ruler_row = choose_ruler_row(
                index_height_no_row, key=lambda x: x[1]
            )
            ruler_row_index = index_size_ruler_row[0]

            y_bounds_ruler_row = get_bounds(blocks[ruler_row_index])[1]
            y_pos_ruler_row = index_pos[ruler_row_index][1][1]

            y_rulers_row.append((curr_row, y_pos_ruler_row))

        for i, blk in enumerate(blocks):
            if use_ruler_pos_col:
                x_bounds = get_bounds(blk)[0]
                if x_pos_ruler_col is not None and is_positioned(
                    blocks[ruler_col_index], x_bounds, None
                ):
                    column_indexes[i] = curr_column
            else:
                if x_bounds_ruler_col is not None and is_positioned(
                    blk, x_bounds_ruler_col, None
                ):
                    column_indexes[i] = curr_column
            if use_ruler_pos_row:
                y_bounds = get_bounds(blk)[1]
                if y_pos_ruler_row is not None and is_positioned(
                    blocks[ruler_row_index], None, y_bounds
                ):
                    rows_indexes[i] = curr_row
            else:
                if y_bounds_ruler_row is not None and is_positioned(
                    blk, None, y_bounds_ruler_row
                ):
                    rows_indexes[i] = curr_row

    sorted_x = sorted(x_rulers_col, key=lambda x: x[1])
    column_mapping = {old_col: new_col for new_col, (old_col, _) in enumerate(sorted_x)}
    # Sort y_ruler_row by their height and create a mapping from old row index to new sorted index
    sorted_y = sorted(y_rulers_row, key=lambda x: x[1])
    row_mapping = {old_row: new_row for new_row, (old_row, _) in enumerate(sorted_y)}
    # Apply the mappings to column_indexes and rows_indexes
    table_coords = list(zip(rows_indexes, column_indexes))
    for i, (row, col) in enumerate(table_coords):
        table_coords[i] = (row_mapping[row], column_mapping[col])

    return table_coords


def is_positioned(
    blk: etree.Element,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if a block is positioned within specified x and/or y ranges.
    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    x_range : Optional[Tuple[float, float]], optional
        Minimum and maximum x-coordinate values (left, right)
    y_range : Optional[Tuple[float, float]], optional
        Minimum and maximum y-coordinate values (bottom, top)
    Returns
    -------
    bool
        True if block's coordinates are within specified ranges, False otherwise
    """
    coords = get_position(blk, True)
    if x_range is not None:
        if not x_range[0] <= coords[0] <= x_range[1]:
            return False

    if y_range is not None:
        if not y_range[0] <= coords[1] <= y_range[1]:
            return False

    return True


def get_lines_contained(
    blk: etree.Element,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> List[etree.Element]:
    lines = blk.findall(".//line")
    return [ln for ln in lines if is_contained(ln, x_range, y_range)]
