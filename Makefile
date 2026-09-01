# Makefile della radice del repository: il cancello che il gancio `.githooks/pre-commit` fa
# scattare a ogni commit.
#
# Il repository e' una raccolta di pacchetti sotto `packages/`, e la copertura vive quasi tutta nel
# crate Rust `packages/freeports` — sia i test unitari (nei `mod tests` dentro `src/`) sia quelli
# d'integrazione (`tests/`, un file per flusso). Gli altri due pacchetti, `freeports_dev` e
# `freeports_validate`, sono Python e oggi non hanno test propri: quando li avranno, il posto in
# cui aggiungerli e' un bersaglio `test-python` qui accanto, non una riga di `pytest` nel gancio.
#
# I test che attraversano il confine verso Python (i moduli `python_boundary`: quelli che aprono
# davvero un PDF con PyMuPDF) girano solo dentro l'ambiente `freeports-dev`; fuori di li' falliscono
# con un messaggio che lo dice. E' la stessa condizione che il gancio pre-commit verifica prima di
# arrivare fin qui.
#
# La documentazione ha il suo Makefile: `make -C docs rustdoc html`.

CARGO    ?= cargo
CRATEDIR  = packages/freeports
MANIFEST  = $(CRATEDIR)/Cargo.toml

.PHONY: help test test-unit test-full check

help:
	@echo "Bersagli disponibili:"
	@echo "  test       la suite completa del crate — quello che gira al commit (alias di test-full)"
	@echo "  test-unit  solo i test unitari, senza i file d'integrazione di $(CRATEDIR)/tests/"
	@echo "  test-full  test unitari + integrazione + doctest"
	@echo "  check      compila senza eseguire nulla, test ed esempi compresi"

# Al commit si esegue tutto: i test d'integrazione sono quelli che accorgerebbero di una
# regressione nel flusso completo, ed e' li' che serve accorgersene, non dopo il push.
test: test-full

test-unit:
	$(CARGO) test --manifest-path $(MANIFEST) --lib

test-full:
	$(CARGO) test --manifest-path $(MANIFEST)

# `--all-targets` include test, esempi e binario: un errore di compilazione in `tests/` o in
# `examples/p0_profile.rs` non si vede compilando la sola libreria.
check:
	$(CARGO) check --manifest-path $(MANIFEST) --all-targets
