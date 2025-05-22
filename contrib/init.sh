REPO_PATH=$(git rev-parse --show-toplevel)

CONTRIB_DIR="${REPO_PATH}/contrib"

git config --local core.hooksPath "${REPO_PATH}/.githooks"

python -m venv "${REPO_PATH}/venv/freeports-dev"
source "${REPO_PATH}/venv/freeports-dev/bin/activate"
pip install --upgrade pip
pip install -r "${CONTRIB_DIR}/requirements.txt"

deactivate