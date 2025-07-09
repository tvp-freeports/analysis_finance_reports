#! /bin/bash
 
REPO_PATH=$(git rev-parse --show-toplevel)

CONTRIB_DIR="${REPO_PATH}/contrib"

git config --local core.hooksPath "${REPO_PATH}/.githooks"
git config --local include.path "${REPO_PATH}/.gitconfig"

python -m venv "${REPO_PATH}/venv/freeports-dev"
source "${REPO_PATH}/venv/freeports-dev/bin/activate"
pip install --upgrade pip
pip install -r "${CONTRIB_DIR}/requirements.minimal.txt"
pip install -r "${CONTRIB_DIR}/requirements.devtools.txt"
pip install -r "${CONTRIB_DIR}/requirements.docs.txt"
pip install -r "${CONTRIB_DIR}/requirements.i18n.txt"
pip install --editable .

deactivate