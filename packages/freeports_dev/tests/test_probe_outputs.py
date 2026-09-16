"""What each shipped probe answers, on a page built to ask it exactly that.

The probes are scripts, run by the tool as separate processes; here they are run in this process with
:func:`runpy.run_path`, which executes the file as ``__main__`` exactly as ``python probe_x.py``
would, without the cost of starting an interpreter per test. The pages are drawn with PyMuPDF, so each
test holds the one layout it is about and nothing else.
"""

import runpy
import sys

import pytest

pymupdf = pytest.importorskip("pymupdf")
pytest.importorskip("freeports.utils.pdf_extract")

from freeports_dev.probe import SHIPPED_DIR  # noqa: E402


def make_pdf(path, pages):
    """A PDF whose pages hold the given ``(x, y, text, fontsize)`` lines."""
    document = pymupdf.open()
    for lines in pages:
        page = document.new_page(width=595, height=842)
        for x, y, text, size in lines:
            page.insert_text((x, y), text, fontsize=size, fontname="helv")
    document.save(path)
    return path


def run_probe(name, *argv, monkeypatch, capsys):
    monkeypatch.setattr(sys, "argv", [f"probe_{name}.py", *map(str, argv)])
    with pytest.raises(SystemExit) as exit_info:
        runpy.run_path(str(SHIPPED_DIR / f"probe_{name}.py"), run_name="__main__")
    return exit_info.value.code, capsys.readouterr().out


class TestEveryProbeWithoutDocuments:
    @pytest.mark.parametrize(
        "name", ["doc_kind", "manco", "assets", "inv_managers", "sfdr_title"]
    )
    def test_prints_its_usage_and_exits_with_2(self, name, monkeypatch, capsys):
        code, out = run_probe(name, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 2 and "Usage:" in out


class TestDocKind:
    def test_the_cover_and_the_first_template_page(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [(50, 80, "ANNUAL REPORT 2024", 20)],
                [(50, 80, "Statement of Net Assets", 12)],
                [
                    (
                        50,
                        80,
                        "Template periodic disclosure for the financial products",
                        10,
                    )
                ],
            ],
        )
        code, out = run_probe("doc_kind", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 0
        row = out.splitlines()[1]
        assert row.split()[:2] == ["3", "3"] and "ANNUAL REPORT 2024" in row

    def test_a_report_without_the_template_says_so(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [[(50, 80, "RELAZIONE SEMESTRALE", 20)]])
        _, out = run_probe("doc_kind", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert out.splitlines()[1].split()[:2] == ["1", "-"]


class TestManco:
    def test_the_value_under_the_label(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (50, 100, "Management Company:", 10),
                    (50, 114, "Waystone Management Company (Lux) S.A.", 9),
                    (50, 126, "19, rue de Bitbourg,", 9),
                ]
            ],
        )
        code, out = run_probe("manco", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 0
        assert "below  -> 'Waystone Management Company (Lux) S.A.'" in out

    def test_a_period_of_office_is_marked(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (50, 100, "MANAGEMENT COMPANY", 9),
                    (50, 114, "Danske Invest Management A/S", 9),
                    (50, 125, "(until 11 June 2024)", 9),
                ]
            ],
        )
        _, out = run_probe("manco", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "'(until 11 June 2024)'" in out and "period of office" in out

    def test_no_label_is_an_answer_not_an_error(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [[(50, 100, "Board of Directors", 10)]])
        code, out = run_probe("manco", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 0 and "no management company label" in out


class TestAssets:
    PAGE = [
        (50, 60, "Planet Impact Global Equities", 10),
        (50, 200, "TOTAL ASSETS", 8),
        (400, 200, "101,518,185", 8),
        (50, 300, "TOTAL LIABILITIES", 8),
        (400, 300, "177,441", 8),
        (50, 320, "TOTAL NET ASSETS", 8),
        (400, 320, "101,340,744", 8),
    ]

    def test_the_three_totals_and_the_equation(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [self.PAGE])
        code, out = run_probe(
            "assets", "en", pdf, monkeypatch=monkeypatch, capsys=capsys
        )
        assert code == 0
        assert "['101,518,185']" in out and "['177,441']" in out
        assert "equation   holds" in out
        assert "Planet Impact Global Equities" in out

    def test_numbers_that_do_not_add_up_are_reported(
        self, tmp_path, monkeypatch, capsys
    ):
        page = [line for line in self.PAGE if line[2] != "101,340,744"] + [
            (400, 320, "1,000", 8)
        ]
        pdf = make_pdf(tmp_path / "r.pdf", [page])
        _, out = run_probe("assets", "en", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "does NOT hold" in out

    def test_the_italian_profile_reads_italian_numbers(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (50, 200, "TOTALE ATTIVITA'", 7),
                    (400, 200, "21.970.695", 7),
                    (50, 300, "TOTALE PASSIVITA'", 7),
                    (400, 300, "274.045", 7),
                    (50, 320, "VALORE COMPLESSIVO NETTO DEL FONDO", 7),
                    (400, 320, "21.696.650", 7),
                ]
            ],
        )
        _, out = run_probe("assets", "it", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "equation   holds" in out

    def test_an_unknown_profile_prints_the_usage(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [self.PAGE])
        code, _ = run_probe("assets", "fr", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 2


class TestInvManagers:
    def test_a_label_and_what_is_under_it(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (300, 100, "Investment Manager and Global Distributor:", 10),
                    (300, 114, "Asteria Obviam SA", 9),
                    (300, 126, "15, rue de Lausanne", 9),
                    (
                        50,
                        400,
                        "Some body text to set the median font size of the page.",
                        9,
                    ),
                ]
            ],
        )
        code, out = run_probe(
            "inv_managers", pdf, monkeypatch=monkeypatch, capsys=capsys
        )
        assert code == 0
        assert "'Investment Manager and Global Distributor:'" in out
        assert "-> Asteria Obviam SA | 15, rue de Lausanne" in out

    def test_a_report_title_is_not_a_label(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf", [[(50, 100, "Investment Manager's Report", 10)]]
        )
        _, out = run_probe("inv_managers", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "no manager label" in out

    def test_a_line_that_merely_ends_with_the_word_investment_is_not_a_label(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (50, 90, "relating to the management of collective", 9),
                    (50, 100, "investment", 9),
                    (50, 112, "undertakings", 9),
                ]
            ],
        )
        _, out = run_probe("inv_managers", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "no manager label" in out

    def test_a_capitalised_label_split_over_two_lines_is_found(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (30, 100, "INVESTMENT", 9),
                    (30, 111, "MANAGER DELEGATI", 9),
                    (200, 100, "Advent Capital Management LLC", 9),
                ]
            ],
        )
        _, out = run_probe("inv_managers", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "'INVESTMENT'" in out

    def test_delegation_in_prose_is_counted(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(
            tmp_path / "r.pdf",
            [
                [
                    (
                        50,
                        100,
                        "The Management Company delegated to Banca Cesare Ponti S.p.A. the day to day",
                        9,
                    ),
                    (
                        50,
                        112,
                        "portfolio management of the above mentioned subfunds.",
                        9,
                    ),
                ]
            ],
        )
        _, out = run_probe("inv_managers", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "delegation in prose: 1 page(s): 1" in out


class TestSfdrTitle:
    def disclosure(self, article, mark_x, labels=("Yes", "No"), mark="X"):
        return [
            (
                50,
                60,
                f"Template periodic disclosure for the financial products referred to in Article {article}",
                9,
            ),
            (
                100,
                150,
                "Did this financial product have a sustainable investment objective?",
                9,
            ),
            (200, 175, labels[0], 9),
            (380, 175, labels[1], 9),
            (mark_x, 175, mark, 9),
        ]

    def test_a_tick_on_no_under_an_article_8_title_agrees(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(tmp_path / "r.pdf", [self.disclosure(8, 365)])
        code, out = run_probe("sfdr_title", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert code == 0
        assert "title Art. 8" in out and "tick Art. 8" in out and "agree" in out

    def test_a_tick_on_yes_is_article_9(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [self.disclosure(9, 185)])
        _, out = run_probe("sfdr_title", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "tick Art. 9" in out and "agree" in out

    def test_a_disagreement_is_shouted(self, tmp_path, monkeypatch, capsys):
        pdf = make_pdf(tmp_path / "r.pdf", [self.disclosure(8, 185)])
        _, out = run_probe("sfdr_title", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "DISAGREE" in out

    def test_labels_in_another_language_than_the_question(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(
            tmp_path / "r.pdf", [self.disclosure(9, 185, labels=("Oui", "Non"))]
        )
        _, out = run_probe("sfdr_title", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "labels Oui/Non" in out and "tick Art. 9" in out

    def test_an_x_fused_with_its_label(self, tmp_path, monkeypatch, capsys):
        page = self.disclosure(9, 0)[:-1]
        page[2] = (185, 175, "X Oui", 9)
        page[3] = (380, 175, "Non", 9)
        pdf = make_pdf(tmp_path / "r.pdf", [page])
        _, out = run_probe("sfdr_title", pdf, monkeypatch=monkeypatch, capsys=capsys)
        assert "tick Art. 9" in out and "agree" in out

    def test_summary_hides_the_disclosures_that_agree(
        self, tmp_path, monkeypatch, capsys
    ):
        pdf = make_pdf(
            tmp_path / "r.pdf", [self.disclosure(8, 365), self.disclosure(8, 185)]
        )
        _, out = run_probe(
            "sfdr_title", "--summary", pdf, monkeypatch=monkeypatch, capsys=capsys
        )
        assert "agree 1" in out and "DISAGREE 1" in out
        assert "p1:" not in out and "p2:" in out
