#!/bin/bash

# One reading of a methodology page, shared by everything that asks it a question.
#
# A page says two quite different things about itself -- which repository paths it applies to
# (`paths.sh`) and which links and images it pins by hash (`links.sh`) -- and both come out of one
# call to `rst_paths.py`. They are kept here rather than in either of those files because a document
# may adopt one methodology and grant fifty files under it, and every one of those questions is
# about the same page: resolving and parsing it once per question would turn a `check-grants` into a
# small denial of service against whoever publishes the methodology.
#
# The answers are left in variables rather than printed. Printing them would mean being called as
# `$(...)`, and a command substitution is a subshell -- the cache would be filled in a process that
# exits immediately afterwards, which is a cache that never hits.
#
# Needs `resolve_methodology` from `sources.sh`.

# Not JSON, and not mistakable for it: what the parser returns is always an object.
readonly PAGE_UNREADABLE="unreadable"

# Where the answers are left.
PAGE_REPORT_JSON=""
PAGE_URI=""
PAGE_PATH=""
PAGE_ORIGIN=""

declare -A PAGE_REPORT_BY_FILE=()
declare -A PAGE_REPORT_BY_NAME=()
declare -A PAGE_URI_BY_NAME=()
declare -A PAGE_PATH_BY_NAME=()
declare -A PAGE_ORIGIN_BY_NAME=()
declare -A PAGE_STATUS_BY_NAME=()

# Parse a page that is already on this machine. Sets `PAGE_REPORT_JSON`.
load_page_report_from_file() {
    local path="$1"
    if [ -n "${PAGE_REPORT_BY_FILE[$path]+set}" ]; then
        PAGE_REPORT_JSON="${PAGE_REPORT_BY_FILE[$path]}"
        return 0
    fi

    local answer
    answer=$(python3 "${LIB_DIR}/rst_paths.py" < "$path") || answer="$PAGE_UNREADABLE"
    PAGE_REPORT_BY_FILE[$path]="$answer"
    PAGE_REPORT_JSON="$answer"
}

# Resolve a methodology by name and parse the page it resolves to.
#
# Sets `PAGE_URI`, `PAGE_PATH`, `PAGE_ORIGIN` and `PAGE_REPORT_JSON`, and returns the resolver's own
# status -- 0, `SOURCE_UNRESOLVED` or `SOURCE_UNREACHABLE` -- because the two failures mean opposite
# things and every caller has to be able to tell them apart.
load_page_report() {
    local methodology="$1"

    if [ -n "${PAGE_STATUS_BY_NAME[$methodology]+set}" ]; then
        PAGE_URI="${PAGE_URI_BY_NAME[$methodology]}"
        PAGE_PATH="${PAGE_PATH_BY_NAME[$methodology]}"
        PAGE_ORIGIN="${PAGE_ORIGIN_BY_NAME[$methodology]}"
        PAGE_REPORT_JSON="${PAGE_REPORT_BY_NAME[$methodology]}"
        return "${PAGE_STATUS_BY_NAME[$methodology]}"
    fi

    PAGE_URI=""
    PAGE_PATH=""
    PAGE_ORIGIN=""
    PAGE_REPORT_JSON="$PAGE_UNREADABLE"

    local resolution status=0
    resolution=$(resolve_methodology "$methodology") || status=$?
    if [ "$status" -eq 0 ]; then
        { read -r PAGE_URI; read -r PAGE_PATH; read -r PAGE_ORIGIN; } <<<"$resolution"
        load_page_report_from_file "$PAGE_PATH"
    fi

    PAGE_URI_BY_NAME[$methodology]="$PAGE_URI"
    PAGE_PATH_BY_NAME[$methodology]="$PAGE_PATH"
    PAGE_ORIGIN_BY_NAME[$methodology]="$PAGE_ORIGIN"
    PAGE_REPORT_BY_NAME[$methodology]="$PAGE_REPORT_JSON"
    PAGE_STATUS_BY_NAME[$methodology]="$status"
    return "$status"
}
