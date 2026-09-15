"""Showing a PDF page on a terminal without pretending the terminal is a page.

The temptation, asked to "print the page", is to draw it: map the page box onto a character grid
and put each line where it falls. It cannot be done honestly. Two lines can overlap and a grid cell
holds one character; a 6pt caption and a 24pt heading occupy the same cell height; and the metadata
that makes a line *selectable* -- its font, its corpus, its box -- does not fit next to the text it
describes. A drawing that is wrong in all three ways is worse than no drawing, because it is read
as if it were right.

So :mod:`freeports_dev.page_view` reports the geometry rather than imitating it, and these tests
hold the three decisions that follow from that:

- **the font legend**. A page has three to eight distinct (font, corpus) pairs and several hundred
  lines. Naming the pair once and tagging each line with a letter costs a short header and gives
  the whole terminal width back to the text -- and the header is exactly what one needs in order to
  write a ``PdfLineSelection``, so it is not a tax;
- **overlap is signalled, never drawn**. A line whose box meets another's is marked. That is the
  fact a reader needs; where the two sit relative to each other is in their coordinates;
- **reading order is a choice the caller makes**. Many pages of these reports carry two tables side
  by side, and sorting by ordinate alone interleaves them into nonsense. Ordering by ordinate is a
  default, not a truth, and the column-major and banded orders are there for the pages it lies
  about.

The functions take anything with ``text``, ``bbox``, ``font_name`` and ``font_size``, so the tests
build lines out of a small record rather than out of a PDF. That is deliberate: what is being
tested is the arrangement, and a test that needs a PDF to check a sort order is a test that will be
skipped the day the PDF moves.
"""

from dataclasses import dataclass

import pytest

from freeports_dev.page_view import (
    ascii_preview,
    font_legend,
    ink_ratio,
    legend_tags,
    nearest_line,
    nearest_lines,
    overlapping_indices,
    reading_order,
    render_lines,
    resample,
    show_codepoints,
)


@dataclass(frozen=True)
class FakeLine:
    """A line of a page, with only what the view functions look at."""

    text: str
    bbox: tuple
    font_name: str = "Calibri"
    font_size: float = 10.0


def line(text, x0, y0, x1=None, y1=None, font="Calibri", size=10.0):
    """A line at a place, with the box filled in from the text length when not given."""
    x1 = x0 + 5.0 * len(text) if x1 is None else x1
    y1 = y0 + size if y1 is None else y1
    return FakeLine(text, (x0, y0, x1, y1), font, size)


class TestTheFontLegend:
    """One entry per distinct (font, corpus) pair, so a line can carry a letter instead."""

    def test_one_entry_per_distinct_font_and_size_pair(self):
        lines = [
            line("a", 0, 0, font="Calibri", size=10.0),
            line("b", 0, 20, font="Calibri", size=10.0),
            line("c", 0, 40, font="Calibri-Bold", size=10.0),
            line("d", 0, 60, font="Calibri", size=12.0),
        ]
        assert [(e.font_name, e.font_size) for e in font_legend(lines)] == [
            ("Calibri", 10.0),
            ("Calibri-Bold", 10.0),
            ("Calibri", 12.0),
        ]

    def test_the_same_font_at_two_sizes_is_two_entries(self):
        lines = [line("a", 0, 0, size=8.0), line("b", 0, 20, size=9.0)]
        assert len(font_legend(lines)) == 2

    def test_entries_are_tagged_in_order_of_first_appearance(self):
        lines = [
            line("a", 0, 0, font="Zed"),
            line("b", 0, 20, font="Alpha"),
            line("c", 0, 40, font="Zed"),
        ]
        assert [e.tag for e in font_legend(lines)] == ["A", "B"]
        assert [e.font_name for e in font_legend(lines)] == ["Zed", "Alpha"]

    def test_more_than_twenty_six_pairs_keep_getting_distinct_tags(self):
        lines = [line("x", 0, 20 * i, size=float(i)) for i in range(30)]
        tags = [e.tag for e in font_legend(lines)]
        assert len(set(tags)) == 30
        assert tags[25:28] == ["Z", "AA", "AB"]

    def test_tags_are_reachable_by_font_and_size(self):
        lines = [
            line("a", 0, 0, font="Calibri", size=10.0),
            line("b", 0, 20, font="X", size=7.0),
        ]
        tags = legend_tags(font_legend(lines))
        assert tags[("Calibri", 10.0)] == "A"
        assert tags[("X", 7.0)] == "B"

    def test_a_page_with_no_lines_has_an_empty_legend(self):
        assert font_legend([]) == []

    def test_sizes_that_differ_below_the_hundredth_are_one_entry(self):
        """PDF corpora arrive as floats with a long tail; 8.980199 and 8.980201 are one corpus."""
        lines = [
            line("a", 0, 0, size=8.980199813842773),
            line("b", 0, 20, size=8.980201),
        ]
        assert len(font_legend(lines)) == 1


class TestReadingOrder:
    """Which order the lines come out in, and why the caller gets to say."""

    def test_by_ordinate_is_top_to_bottom_then_left_to_right(self):
        lines = [line("c", 50, 100), line("a", 10, 10), line("b", 90, 10)]
        assert [item.text for item in reading_order(lines, order="y")] == [
            "a",
            "b",
            "c",
        ]

    def test_by_ordinate_treats_a_hairline_difference_as_the_same_row(self):
        """231.9 and 232.0 are one row of one table, and must not swap the two columns."""
        lines = [line("right", 300, 232.0), line("left", 100, 231.9)]
        assert [item.text for item in reading_order(lines, order="y")] == [
            "left",
            "right",
        ]

    def test_by_abscissa_reads_down_each_column_first(self):
        lines = [
            line("l1", 10, 10),
            line("r1", 300, 10),
            line("l2", 10, 50),
            line("r2", 300, 50),
        ]
        assert [item.text for item in reading_order(lines, order="x")] == [
            "l1",
            "l2",
            "r1",
            "r2",
        ]

    def test_two_bands_read_the_left_table_whole_before_the_right_one(self):
        """The case the default lies about: two tables side by side on one page."""
        lines = [
            line("left-1", 20, 100),
            line("right-1", 320, 100),
            line("left-2", 20, 200),
            line("right-2", 320, 200),
        ]
        ordered = reading_order(lines, order="y", columns=2, page_width=600.0)
        assert [item.text for item in ordered] == [
            "left-1",
            "left-2",
            "right-1",
            "right-2",
        ]

    def test_a_single_band_is_the_plain_order(self):
        lines = [line("b", 300, 10), line("a", 10, 10)]
        assert reading_order(
            lines, order="y", columns=1, page_width=600.0
        ) == reading_order(lines, order="y")

    def test_the_page_width_is_taken_from_the_lines_when_not_given(self):
        lines = [line("left", 0, 10, x1=10), line("right", 90, 10, x1=100)]
        ordered = reading_order(lines, order="y", columns=2)
        assert [item.text for item in ordered] == ["left", "right"]

    def test_an_unknown_order_is_an_error_rather_than_a_silent_default(self):
        with pytest.raises(ValueError, match="sideways"):
            reading_order([line("a", 0, 0)], order="sideways")

    def test_ordering_no_lines_yields_no_lines(self):
        assert reading_order([], order="y") == []


class TestOverlapIsSignalled:
    """A box meeting another box is a fact worth marking, and not one worth drawing."""

    def test_two_boxes_that_meet_are_both_marked(self):
        lines = [line("a", 10, 10, x1=100, y1=20), line("b", 50, 12, x1=150, y1=18)]
        assert overlapping_indices(lines) == {0, 1}

    def test_boxes_that_only_touch_at_an_edge_do_not_count(self):
        lines = [line("a", 10, 10, x1=100, y1=20), line("b", 100, 10, x1=200, y1=20)]
        assert overlapping_indices(lines) == set()

    def test_boxes_on_different_rows_do_not_count(self):
        lines = [line("a", 10, 10, x1=100, y1=20), line("b", 10, 30, x1=100, y1=40)]
        assert overlapping_indices(lines) == set()

    def test_one_box_inside_another_counts(self):
        lines = [line("a", 0, 0, x1=200, y1=100), line("b", 10, 10, x1=20, y1=20)]
        assert overlapping_indices(lines) == {0, 1}

    def test_only_the_boxes_that_meet_are_marked(self):
        lines = [
            line("a", 10, 10, x1=100, y1=20),
            line("b", 50, 12, x1=150, y1=18),
            line("far", 10, 500, x1=100, y1=510),
        ]
        assert overlapping_indices(lines) == {0, 1}

    def test_a_page_of_one_line_has_no_overlaps(self):
        assert overlapping_indices([line("a", 0, 0)]) == set()


class TestCodepoints:
    """What tells a box glyph from a letter."""

    def test_plain_ascii_is_left_alone(self):
        assert show_codepoints("Total assets") == "Total assets"

    def test_a_non_ascii_character_is_shown_as_its_codepoint(self):
        assert show_codepoints("a⚫b") == "a<U+26AB>b"

    def test_the_box_glyphs_are_the_point_of_this(self):
        assert show_codepoints("☒") == "<U+2612>"

    def test_an_empty_string_stays_empty(self):
        assert show_codepoints("") == ""


class TestRenderingTheTable:
    """What actually reaches the terminal."""

    def test_the_legend_comes_before_the_lines(self):
        out = render_lines(
            [line("Total assets", 48.9, 102.5, font="frutiger-black", size=8.98)]
        )
        assert out[0].startswith("fonts")
        assert any("frutiger-black" in row for row in out[:4])
        assert any("Total assets" in row for row in out)

    def test_a_line_carries_its_tag_and_not_its_font_name(self):
        out = render_lines(
            [
                line("one", 10, 10, font="averyveryverylongfontname", size=9.0),
                line("two", 10, 30, font="averyveryverylongfontname", size=9.0),
            ]
        )
        body = [row for row in out if "one" in row or "two" in row]
        assert len(body) == 2
        assert all("averyveryverylongfontname" not in row for row in body)
        assert all(" A " in row for row in body)

    def test_the_text_is_truncated_to_the_width_asked_for(self):
        out = render_lines([line("x" * 200, 0, 0)], text_width=20)
        body = [row for row in out if "xxx" in row][0]
        assert "x" * 20 in body
        assert "x" * 21 not in body

    def test_an_overlapping_line_is_marked_and_the_marker_is_explained(self):
        lines = [line("a", 10, 10, x1=100, y1=20), line("b", 50, 12, x1=150, y1=18)]
        out = render_lines(lines)
        marked = [row for row in out if row.rstrip().endswith("!")]
        assert len(marked) == 2
        assert any("overlap" in row.lower() for row in out)

    def test_codepoints_are_shown_only_when_asked_for(self):
        assert not any("U+26AB" in row for row in render_lines([line("⚫", 0, 0)]))
        assert any(
            "U+26AB" in row
            for row in render_lines(
                [
                    line("⚫", 0, 0),
                ],
                codepoints=True,
            )
        )

    def test_the_order_asked_for_is_named_in_the_output(self):
        out = render_lines([line("a", 0, 0)], order="x")
        assert any("order" in row.lower() and "x" in row for row in out[:4])

    def test_a_page_with_no_lines_says_so_rather_than_printing_nothing(self):
        out = render_lines([])
        assert out
        assert any("no lines" in row.lower() for row in out)


class TestImagePreview:
    """An image on a terminal, which unlike a page really can be drawn."""

    def test_a_blank_image_previews_as_blank(self):
        gray = [255] * 16
        assert all(row.strip() == "" for row in ascii_preview(gray, 4, 4, cols=4))

    def test_a_black_image_previews_as_solid_ink(self):
        gray = [0] * 16
        assert all(set(row) == {"@"} for row in ascii_preview(gray, 4, 4, cols=4))

    def test_the_preview_is_as_wide_as_asked_and_keeps_the_aspect_ratio(self):
        gray = [0] * (40 * 20)
        preview = ascii_preview(gray, 40, 20, cols=20)
        assert all(len(row) == 20 for row in preview)
        # Half as tall as wide in the image; halved again because a character cell is about twice
        # as tall as it is wide.
        assert len(preview) == 5

    def test_a_preview_is_never_taller_than_one_row(self):
        assert len(ascii_preview([0], 1, 1, cols=8)) >= 1

    def test_ink_ratio_is_nothing_for_white_and_everything_for_black(self):
        assert ink_ratio([255] * 9) == 0.0
        assert ink_ratio([0] * 9) == 1.0

    def test_ink_ratio_of_half_black_is_a_half(self):
        assert ink_ratio([0] * 8 + [255] * 8) == pytest.approx(0.5)

    def test_ink_ratio_of_nothing_is_nothing_rather_than_an_error(self):
        assert ink_ratio([]) == 0.0

    def test_resampling_shrinks_by_averaging_rather_than_by_dropping(self):
        gray = [0, 0, 255, 255, 0, 0, 255, 255]
        assert resample(gray, 4, 2, 2, 1) == [[0, 255]]

    def test_resampling_to_the_same_size_changes_nothing(self):
        assert resample([1, 2, 3, 4], 2, 2, 2, 2) == [[1, 2], [3, 4]]


class TestNearestText:
    """What an image *means* is whatever is written next to it."""

    def test_the_closest_line_is_the_one_returned(self):
        lines = [line("far", 10, 10), line("near", 360, 231, x1=380, y1=241)]
        found = nearest_line((341.0, 232.4, 350.1, 241.5), lines)
        assert found.line.text == "near"

    def test_a_line_to_the_right_is_reported_as_being_to_the_right(self):
        lines = [line("No", 362.4, 231.9, x1=375, y1=242)]
        assert nearest_line((341.0, 232.4, 350.1, 241.5), lines).direction == "right"

    def test_a_line_to_the_left_is_reported_as_being_to_the_left(self):
        lines = [line("Yes", 184.1, 231.9, x1=200, y1=242)]
        assert nearest_line((341.0, 232.4, 350.1, 241.5), lines).direction == "left"

    def test_a_line_above_is_reported_as_being_above(self):
        lines = [line("Heading", 340, 100, x1=400, y1=112)]
        assert nearest_line((341.0, 232.4, 350.1, 241.5), lines).direction == "above"

    def test_a_line_below_is_reported_as_being_below(self):
        lines = [line("Caption", 340, 400, x1=400, y1=412)]
        assert nearest_line((341.0, 232.4, 350.1, 241.5), lines).direction == "below"

    def test_a_page_with_no_text_yields_nothing_rather_than_failing(self):
        assert nearest_line((0.0, 0.0, 1.0, 1.0), []) is None

    def test_several_neighbours_come_back_nearest_first(self):
        lines = [
            line("far", 10, 10),
            line("near", 360, 231, x1=380, y1=241),
            line("mid", 200, 230),
        ]
        assert [
            n.line.text for n in nearest_lines((341.0, 232.4, 350.1, 241.5), lines)
        ] == [
            "near",
            "mid",
            "far",
        ]

    def test_asking_for_more_neighbours_than_the_page_has_yields_what_there_is(self):
        assert (
            len(nearest_lines((0.0, 0.0, 1.0, 1.0), [line("only", 50, 50)], count=3))
            == 1
        )

    def test_the_single_nearest_is_the_first_of_the_several(self):
        """The heading a tick sits under is nearer than its own label, which is why three are shown."""
        lines = [
            line("heading", 340, 220, x1=520, y1=232),
            line("No", 362.4, 231.9, x1=375, y1=242),
        ]
        box = (341.0, 232.4, 350.1, 241.5)
        assert nearest_line(box, lines).line.text == "heading"
        assert [n.line.text for n in nearest_lines(box, lines)] == ["heading", "No"]
