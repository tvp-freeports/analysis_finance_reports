#!/bin/bash

# The resources a methodology page cites, and whether they still say what it pinned them saying.
#
# A methodology page is prose, and prose cites things: a diagram, a specification, another document.
# A grant made under that page is in part a claim about what those things said, so the page may pin
# each of them by hash in an ordinary reStructuredText comment -- see `rst_paths.py` for the shape.
#
# **The pins are not a second trust root.** They are lines of the page, so the page's own `sha256` --
# the one every validation document records -- already commits to every one of them. Checking them
# is therefore an *optional deepening* and never a separate thing to trust, which is why it happens
# only when `--deep` (or `validate.deep`) asks for it: a page whose pins were never fetched is not
# thereby less trustworthy than its hash already says it is.
#
# The same three-valued answer as everywhere else in this command. A pin that disagrees is a
# finding; a pin whose target could not be fetched is the *absence* of a finding, and reporting it
# as either a match or a mismatch would be a lie about work nobody did.
#
# Needs `fetch_uri`, `cached_body_for`, `store_in_cache` and `local_path_of` from `sources.sh`, and
# `load_page_report_from_file` from `page.sh`.

readonly LINK_MATCH=0
readonly LINK_MISMATCH=1
readonly LINK_UNREACHABLE=2

# Where a cited target actually lives.
#
# An absolute URI stands for itself. Anything else is a path relative to *the page that cites it* --
# not to the working directory, which the page knows nothing about, and not to the repository root,
# which a page published on somebody else's website does not have. That is what lets one page carry
# `assets/pipeline.svg` and mean the right file whether it is read from a checkout or fetched from
# the published documentation.
link_target_uri() {
    local target="$1" page_uri="$2"
    case "$target" in
        *://*) echo "$target"; return 0 ;;
    esac
    echo "${page_uri%/*}/${target}"
}

# A local path holding the bytes a target names, or nothing and `LINK_UNREACHABLE`.
#
# The order of preference is the resolver's, deliberately: online, the network is asked first and
# the cache is only a fallback. A cache consulted first would quietly pin a resource to whatever was
# fetched the day it was first seen, and the cited document changing is exactly the event the pins
# exist to notice.
resolve_link_body() {
    local uri="$1"

    local path
    path=$(local_path_of "$uri")
    if [ -n "$path" ]; then
        [ -f "$path" ] || return $LINK_UNREACHABLE
        echo "$path"
        return $LINK_MATCH
    fi

    local cached
    cached=$(cached_body_for "$uri")

    if [ -n "$FREEPORTS_VALIDATE_OFFLINE" ]; then
        [ -n "$cached" ] || return $LINK_UNREACHABLE
        echo "$cached"
        return $LINK_MATCH
    fi

    local fetched status=0
    fetched=$(mktemp "${TMPDIR:-/tmp}/freeports-validate-link.XXXXXX") || return $LINK_UNREACHABLE
    fetch_uri "$uri" "$fetched" || status=$?
    if [ "$status" -eq 0 ]; then
        local stored
        stored=$(store_in_cache "$uri" "$fetched")
        rm -f "$fetched"
        echo "$stored"
        return $LINK_MATCH
    fi
    rm -f "$fetched"

    [ -n "$cached" ] || return $LINK_UNREACHABLE
    echo "$cached"
    return $LINK_MATCH
}

# The hash a target has right now, for the page that cites it.
link_target_sha256() {
    local target="$1" page_uri="$2"
    local uri body status=0
    uri=$(link_target_uri "$target" "$page_uri")
    body=$(resolve_link_body "$uri") || status=$?
    [ "$status" -eq 0 ] || return $status
    sha256_of_file "$body"
}

# Every target a page cites, pinned or not, one per line and in the page's own order.
page_cited_targets() {
    local json="$1"
    echo "$json" | jq -r '(.links.pinned[].target), (.links.unpinned[])'
}

# What a page cites, and -- when `deep` is true -- whether each pin still holds.
#
# Returns 0 unless a pin disagreed. An unreachable target never fails the run: it is warned about,
# explained at the end, and counted nowhere.
check_page_links() {
    local page_path="$1" page_uri="$2" deep="$3"

    load_page_report_from_file "$page_path"
    local json="$PAGE_REPORT_JSON"
    if [ "$json" = "$PAGE_UNREADABLE" ]; then
        print_warning "  The page could not be read, so what it cites is unknown"
        return 0
    fi

    local pinned unpinned stale
    pinned=$(echo "$json" | jq '.links.pinned | length')
    unpinned=$(echo "$json" | jq '.links.unpinned | length')
    stale=$(echo "$json" | jq '.links.stale | length')

    echo "  pinned resources:          $pinned"
    echo "  unpinned links and images: $unpinned"

    # Not an error and not counted: a pin naming something the page has stopped citing is a claim
    # left behind by an edit, and the person who made the edit is the one who can say whether the
    # claim went with it. `refresh-links` removes them.
    if [ "$stale" -gt 0 ]; then
        print_warning "  pins naming something the page no longer cites: $stale"
    fi

    [ "$deep" = "true" ] || return 0
    [ "$pinned" -eq 0 ] && return 0

    local index target expected actual status failures=0
    for ((index = 0; index < pinned; index++)); do
        target=$(echo "$json" | jq -r ".links.pinned[$index].target")
        expected=$(echo "$json" | jq -r ".links.pinned[$index].sha256")

        status=0
        actual=$(link_target_sha256 "$target" "$page_uri") || status=$?
        if [ "$status" -ne 0 ]; then
            print_warning "  $target: could not be reached, so nothing was compared"
            note_diagnosis subhash-unreachable
            continue
        fi

        if [ "$actual" = "$expected" ]; then
            print_success "  $target matches what the page pins it at"
        else
            print_error "  $target is not what the page pins it at"
            note_diagnosis subhash-mismatch
            failures=$((failures + 1))
        fi
    done

    [ "$failures" -eq 0 ]
}

# The same, for a methodology named rather than a page pointed at. Silent about a page that could
# not be resolved, because the check that resolves it has already said so on its own line.
verify_methodology_links() {
    local methodology="$1"
    local status=0
    load_page_report "$methodology" || status=$?
    [ "$status" -eq 0 ] || return 0
    print_info "Pinned resources of \"$methodology\""
    check_page_links "$PAGE_PATH" "$PAGE_URI" "true"
}
