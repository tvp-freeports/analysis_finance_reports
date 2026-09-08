#!/bin/bash

# Whether a granted path is one its methodology says it covers.
#
# A methodology page may declare the repository paths it applies to, in a `Supported paths` section
# -- see `rst_paths.py` for the shape of one. What that declaration is *for* is the sentence under
# each pattern: `tests/formats/` means one thing in a formats repository and another in the engine's
# own, and the ambiguity is resolved by an author writing which they mean, not by the tool guessing.
# So nothing here interprets the prose; everything here shows it.
#
# The three moments a declaration is consulted are deliberately not symmetric:
#
#   grant          refuses, and the refusal can be overridden -- catching the mistake while its
#                  author is standing there is cheap, and being wrong about it costs them one flag.
#   check-grants   warns, and never fails. A repository must not go red because somebody else
#                  edited a page it adopts.
#   the lookups    list the grant and mark it. Somebody made that claim; the reader is told under
#                  what.
#
# **A page with no such section supports any path.** That is the behaviour from before the section
# existed, and it is the honest answer for a methodology -- `agreement and good faith` is the
# example -- whose scope genuinely cannot be written as a set of paths.
#
# Needs `load_page_report` from `page.sh` -- which is where the page is resolved and parsed, once
# per run -- and `print_warning` from `validation_utils.sh`.

# The three answers `path_is_supported` gives, and the reason there are three rather than two: the
# page is what says which paths are declared, so a page that could not be fetched leaves the
# question *unasked*. Reporting that as "supported" would vouch for a scope nobody read, and
# reporting it as "unsupported" would accuse a granter over a network failure.
readonly PATH_SUPPORTED=0
readonly PATH_UNSUPPORTED=1
readonly PATH_UNDECIDABLE=2

# The mark a lookup appends to an out-of-scope grant, and the one line that explains it. A symbol
# rather than a word because it is appended to items in a list, and a legend rather than a repeated
# sentence because a document may hold a hundred of them.
readonly OUT_OF_SCOPE_MARK="(!)"
readonly OUT_OF_SCOPE_LEGEND="(!) granted outside the paths its methodology declares it supports."

# Does this methodology declare the path it was granted against?
#
#   0  yes -- or it declares no paths at all, which means it covers any of them
#   1  no: it declares paths, and this is not one of them
#   2  the page could not be resolved, so the question could not be asked
path_is_supported() {
    local methodology="$1" relative="$2"

    local json
    load_page_report "$methodology"
    json="$PAGE_REPORT_JSON"
    [ "$json" = "$PAGE_UNREADABLE" ] && return $PATH_UNDECIDABLE

    local declared
    declared=$(echo "$json" | jq -r '.supported_paths.declared')
    [ "$declared" = "true" ] || return $PATH_SUPPORTED

    local matched
    matched=$(echo "$json" | jq -r '.supported_paths.patterns[].pattern' \
        | python3 "${LIB_DIR}/pathmatch.py" filter "$relative" \
        | jq -r '.[0].matched')
    [ "$matched" = "null" ] && return $PATH_UNSUPPORTED
    return $PATH_SUPPORTED
}

#: Filled by `unsupported_paths` with the paths the methodology does not cover, in the order they
#: were asked about. Empty is the good answer, and the usual one.
UNSUPPORTED_PATHS=()

# The same question as `path_is_supported`, asked about many paths at once.
#
# `pathmatch.py filter` was written to take a whole list -- its own docstring says so, in as many
# words: a document may grant hundreds of files under one methodology, and starting an interpreter
# per file to answer a yes-or-no question is a cost with nothing behind it. Asking one path at a
# time is what the caller above does, and what a caller holding the whole list must not do.
#
# Returns `PATH_UNDECIDABLE` when the page could not be resolved -- a fact about the methodology
# and not about any one path, so it is reported once rather than repeated for every file -- and 0
# otherwise, with the offending paths left in `UNSUPPORTED_PATHS`.
unsupported_paths() {
    local methodology="$1"
    shift

    UNSUPPORTED_PATHS=()
    [ "$#" -gt 0 ] || return $PATH_SUPPORTED

    local json
    load_page_report "$methodology"
    json="$PAGE_REPORT_JSON"
    [ "$json" = "$PAGE_UNREADABLE" ] && return $PATH_UNDECIDABLE

    local declared
    declared=$(echo "$json" | jq -r '.supported_paths.declared')
    [ "$declared" = "true" ] || return $PATH_SUPPORTED

    mapfile -t UNSUPPORTED_PATHS < <(
        echo "$json" | jq -r '.supported_paths.patterns[].pattern' \
            | python3 "${LIB_DIR}/pathmatch.py" filter "$@" \
            | jq -r '.[] | select(.matched == null) | .path'
    )
    return $PATH_SUPPORTED
}

# Every pattern the methodology declares, with the prose that says what vouching for it means.
#
# The prose is the whole point of printing this. "That path is not supported" tells a reader nothing
# they can act on; "this methodology covers the reference output of a test suite, and yours is a
# page fixture" tells them whether they picked the wrong file or the wrong methodology.
print_supported_paths() {
    local methodology="$1"

    local json
    load_page_report "$methodology"
    json="$PAGE_REPORT_JSON"
    if [ "$json" = "$PAGE_UNREADABLE" ]; then
        echo "    (the page could not be resolved, so its declaration could not be read)"
        return 0
    fi

    local count
    count=$(echo "$json" | jq '.supported_paths.patterns | length')
    if [ "$count" -eq 0 ]; then
        echo "    The page has a \"Supported paths\" section, and it declares no path at all."
        echo "    That is almost certainly a mistake in the page rather than a methodology that"
        echo "    covers nothing; its author is the person who can say."
        return 0
    fi

    local index pattern prose
    for ((index = 0; index < count; index++)); do
        pattern=$(echo "$json" | jq -r ".supported_paths.patterns[$index].pattern")
        prose=$(echo "$json" | jq -r ".supported_paths.patterns[$index].prose")
        echo "    $pattern"
        [ -n "$prose" ] && printf '%s\n' "$prose" | sed 's/^/        /'
    done
}

# `grant`: may this path be granted under this methodology? Prints the refusal when it may not.
#
# Two quite different reasons to stop, and both are overridable, because both are a judgement the
# person signing is entitled to make and neither is one the tool can make for them. What is not
# negotiable is that they are told: a refusal overridden in silence would leave the declaration
# unenforceable *and* invisible, which is worse than not having one.
require_supported_path() {
    local methodology="$1" relative="$2" force="$3"

    local status=0
    path_is_supported "$methodology" "$relative" || status=$?
    [ "$status" -eq "$PATH_SUPPORTED" ] && return 0

    if [ "$status" -eq "$PATH_UNDECIDABLE" ]; then
        print_error "Methodology \"$methodology\" could not be resolved, so what it covers is unknown"
        echo "A grant is a claim made under a text. This run could not read that text, so it" >&2
        echo "cannot tell you whether \"$relative\" is a file the methodology applies to -- and" >&2
        echo "signing under a page you could not read is exactly what a signature must not mean." >&2
        echo "\`freeports-validate sources\` shows what your sources resolve." >&2
    else
        print_error "\"$relative\" is not a path methodology \"$methodology\" says it supports"
        echo "The methodology declares which paths it covers, and what vouching for each of them" >&2
        echo "means:" >&2
        print_supported_paths "$methodology" >&2
        echo "" >&2
        echo "If none of those is what you have, the file probably wants a different methodology" >&2
        echo "-- or the methodology's own page wants a pattern it does not yet have, which is a" >&2
        echo "conversation with its author rather than a flag." >&2
    fi

    if [ "$force" = "true" ]; then
        print_warning "--force given: granting \"$relative\" anyway"
        return 0
    fi

    # Only when a person is there. A question nobody can answer must not become a default yes, so
    # under a pipe -- continuous integration, a script, a here-document -- this is simply a refusal.
    if [ -t 0 ]; then
        local answer
        read -r -p "Grant it anyway? [y/N] " answer
        case "$answer" in
            [yY] | [yY][eE][sS]) return 0 ;;
        esac
        print_error "Not granted: \"$relative\""
        return 1
    fi

    echo "" >&2
    echo "Nothing was granted. Pass --force to grant it anyway, or run this from a terminal to be" >&2
    echo "asked." >&2
    return 1
}

# `check-grants`: say so, and nothing more.
#
# Always returns success. The caller must not be able to turn this into a failure by accident, and
# the reason is in the plan: a repository failing its own checks because somebody else edited a
# methodology page would make adopting anybody's methodology a liability.
report_granted_path() {
    local methodology="$1" relative="$2" known_status="${3:-}"

    # The third argument is the answer, when the caller asked it for many paths at once. The
    # question is the same one; what changes is that a document granting several hundred files asks
    # it once rather than once per file. A caller with nothing to pass asks here, as before.
    local status=0
    if [ -n "$known_status" ]; then
        status="$known_status"
    else
        path_is_supported "$methodology" "$relative" || status=$?
    fi
    [ "$status" -eq "$PATH_UNSUPPORTED" ] || return 0

    print_warning "File \"$relative\" is outside the paths methodology \"$methodology\" declares"
    note_diagnosis path-unsupported
    note_unsupported_methodology "$methodology"
    return 0
}

# The lookups: annotate, do not hide.
#
# Sets `MARK` in the caller's own shell rather than printing it, because the caller needs two things
# out of one question -- the mark for this line, and the knowledge that a legend is now owed at the
# bottom -- and a command substitution would put both in a subshell that then exits, taking the
# resolved-page cache with it.
OUT_OF_SCOPE_SEEN=false

set_out_of_scope_mark() {
    local methodology="$1" relative="$2"

    MARK=""
    local status=0
    path_is_supported "$methodology" "$relative" || status=$?
    if [ "$status" -eq "$PATH_UNSUPPORTED" ]; then
        MARK=" $OUT_OF_SCOPE_MARK"
        OUT_OF_SCOPE_SEEN=true
    fi
}

print_out_of_scope_legend() {
    [ "$OUT_OF_SCOPE_SEEN" = true ] || return 0
    echo ""
    echo "$OUT_OF_SCOPE_LEGEND"
}
