#!/bin/bash

# Where a methodology's *text* comes from.
#
# The pages used to ship with this command, and a grant meant whatever the installed copy said.
# They no longer do: a grant is made under a text resolved from a **source**, and the sources are
# named by the person running the command. That makes the source an explicit contract between the
# granter and whoever later verifies the grant -- the document records only a name and a hash, and
# the text those refer to is the one each of them configured.
#
# A source is a pattern containing exactly one `*`. The `*` stands for the methodology's own
# relative name, which may itself contain a slash:
#
#     https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
#     https://github.com/tvp-freeports/analysis_finance_reports/blob/main/docs/source/validation/*.rst
#     file:///home/me/my-methodologies/*.rst
#     ../my-methodologies/*.rst
#
# `basic check` becomes `methodologies/basic_check`, the general methodology becomes
# `general_methodology`, and the pattern says the rest. One rule covers the published documentation
# and a checked-out repository alike, which is why the grammar is this small.
#
# Everything below is a command you can retype. That is the point of it being shell: the fetch is
# `curl`, the hash is `sha256sum`, the substitution is a parameter expansion, and
# `freeports-validate sources` prints what each of them resolves so that "why does my hash differ
# from yours" has an answer you can check yourself.
#
# Needs `print_warning` and `print_error`, so source `validation_utils.sh` first.

# The sources in priority order, one per line, as `cli.py` resolved them. Newline is the separator
# because it is the one character a URI and a path cannot contain: nothing here has to be escaped,
# and nothing can be split wrongly.
readarray -t VALIDATE_SOURCES <<<"${FREEPORTS_VALIDATE_SOURCES}"

# Content-addressed, and outside the repository: what is cached is somebody else's published text,
# not this project's work, and it belongs with the user's other caches.
SOURCE_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/freeports-validate"

# Printed once per run rather than once per fetch. A run touches every adopted methodology, and the
# fact worth telling the reader -- that these texts arrived over an unauthenticated channel -- is one
# fact about the run, not one fact per page.
HTTP_WARNING_GIVEN=false

# `resolve_methodology` answers with three lines and an exit status. The statuses are separate
# because the two failures mean opposite things to a reader: a name nothing offers is a
# misconfiguration or a withdrawn methodology, while a source that cannot be reached is the absence
# of an answer -- neither a match nor a mismatch -- and reporting it as either would be a lie.
readonly SOURCE_OK=0
readonly SOURCE_UNRESOLVED=1
readonly SOURCE_UNREACHABLE=2

# curl's own vocabulary for the same distinction. `-f` makes an HTTP status of 400 or more an error,
# and reports it as 22; every other failure is about the connection rather than about the document.
readonly CURL_HTTP_ERROR=22

sha256_of_string() {
    printf '%s' "$1" | sha256sum | awk '{print $1}'
}

sha256_of_file() {
    sha256sum "$1" | awk '{print $1}'
}

# A source must contain exactly one `*`. Zero and two are both refused rather than guessed at:
# with none there is nothing to substitute, and with two there is no way to tell which half is the
# base and which the extension. Either way the fix is a line in the user's configuration, so the
# message names the offending source verbatim.
validate_source_pattern() {
    local pattern="$1"
    local stars="${pattern//[^\*]/}"

    if [ "${#stars}" -eq 1 ]; then
        return 0
    fi

    print_error "Not a usable methodology source: \"$pattern\""
    echo "A source is a pattern containing exactly one '*', which stands for the methodology's" >&2
    echo "name -- 'general_methodology', or 'methodologies/basic_check'. For example:" >&2
    echo "    https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt" >&2
    echo "    ../my-methodologies/*.rst" >&2
    if [ "${#stars}" -eq 0 ]; then
        echo "This one has none, so there is nothing for a methodology's name to replace." >&2
    else
        echo "This one has ${#stars}, and which of them is the name cannot be guessed." >&2
    fi
    echo "Set it with --source, FREEPORTS_VALIDATE_SOURCE, or validate.sources in the" >&2
    echo "freeports configuration file." >&2
    return 1
}

require_sources() {
    if [ "${#VALIDATE_SOURCES[@]}" -eq 0 ] || [ -z "${VALIDATE_SOURCES[0]}" ]; then
        print_error "No methodology source is configured"
        echo "Methodology pages do not ship with this command: a grant is made under a text, and" >&2
        echo "a source says which text. Name one with --source, FREEPORTS_VALIDATE_SOURCE, or" >&2
        echo "validate.sources in the freeports configuration file." >&2
        return 1
    fi
    local source
    for source in "${VALIDATE_SOURCES[@]}"; do
        validate_source_pattern "$source" || return 1
    done
    return 0
}

# A methodology's name as it is stored in a validation document -- lowercased, spaces for
# underscores -- turned back into the relative path a source pattern expects.
methodology_relative_name() {
    local name
    name=$(echo "$1" | tr '[:upper:]' '[:lower:]' | tr ' ' '_')
    echo "methodologies/${name}"
}

general_methodology_relative_name() {
    echo "general_methodology"
}

# GitHub serves two different things at two different URLs, and only one of them is the document.
# `/blob/` is the web page *about* the file: fetching it would hash a page of HTML, and the hash
# would be stable enough that nobody would notice for a long time. So a `blob` URL is translated to
# the `raw.githubusercontent.com` one that serves the bytes.
#
# This is a translation and not a compatibility shim: both spellings are correct input, and neither
# is deprecated. A `raw.githubusercontent.com` URL given directly is already right and is left alone.
rewrite_github_url() {
    local uri="$1"
    if [[ "$uri" =~ ^https://github\.com/([^/]+)/([^/]+)/blob/(.+)$ ]]; then
        echo "https://raw.githubusercontent.com/${BASH_REMATCH[1]}/${BASH_REMATCH[2]}/${BASH_REMATCH[3]}"
    else
        echo "$uri"
    fi
}

# The `*` replaced by a relative name. Written as two parameter expansions rather than `sed`,
# because a URI is full of characters `sed` would want escaped and a pattern is not a regular
# expression.
source_uri_for() {
    local pattern="$1" relative="$2"
    rewrite_github_url "${pattern%%\**}${relative}${pattern#*\*}"
}

# Every URI a name could come from, in priority order, without fetching any of them. This is what
# `sources` prints and what the hash-mismatch diagnosis shows: where the text *might* have come
# from is answerable on paper, and answering it should not need a network.
candidate_uris() {
    local relative="$1" source
    for source in "${VALIDATE_SOURCES[@]}"; do
        source_uri_for "$source" "$relative"
    done
}

# A URI naming a file on this machine, as a plain path -- or nothing, if it names something remote.
#
# The answer is absolute even when the source was written relative to the shell it was typed in. The
# URI line keeps what the user wrote, because that is the thing they can retype; the path line is
# handed to callers that will `sha256sum` it from wherever they happen to be standing, and a path
# that depended on the caller's working directory would break the first time one of them changed it.
local_path_of() {
    local uri="$1" path
    case "$uri" in
        file://*) path="${uri#file://}" ;;
        *://*)    echo ""; return 0 ;;
        *)        path="$uri" ;;
    esac
    # `-m` so a name that does not exist yet still comes back absolute: whether the page is there is
    # the caller's question, and answering it here would conflate "not a local source" with "not
    # a page this source has".
    realpath -m "$path"
}

cache_dir_for() {
    echo "${SOURCE_CACHE}/$(sha256_of_string "$1")"
}

# Every distinct body ever seen for a URI is kept, named by its own hash, and `latest` names the
# most recent. Keeping the old ones is what makes a mismatch auditable rather than merely reported:
# the text a grant was made under is still here to be read, next to the text that replaced it.
store_in_cache() {
    local uri="$1" body_file="$2"
    local dir digest
    dir=$(cache_dir_for "$uri")
    digest=$(sha256_of_file "$body_file")
    mkdir -p "$dir" || return 1
    if [ ! -f "${dir}/${digest}" ]; then
        cp "$body_file" "${dir}/${digest}" || return 1
    fi
    ln -sfn "$digest" "${dir}/latest" || return 1
    echo "${dir}/${digest}"
}

cached_body_for() {
    local dir
    dir=$(cache_dir_for "$1")
    [ -e "${dir}/latest" ] && echo "$(cd "$dir" && pwd)/$(readlink "${dir}/latest")"
}

warn_about_http_once() {
    [ "$HTTP_WARNING_GIVEN" = true ] && return 0
    HTTP_WARNING_GIVEN=true
    print_warning "Fetching methodologies over unencrypted http"
    echo "    The hash in the validation document is what compensates for an unauthenticated" >&2
    echo "    fetch, so this is not a hole in the mechanism -- but you should know which kind of" >&2
    echo "    fetch you are doing. Use https where the source offers it." >&2
}

# Fetch one URI into a temporary file, and say which kind of failure it was.
#
# `-f` turns an HTTP status of 400 or more into a failure, and that is the distinction the whole
# resolver rests on: a 404 is the source *answering* that it does not have this name, while a
# refused connection or a timeout is no answer at all.
fetch_uri() {
    local uri="$1" destination="$2"
    case "$uri" in
        http://*) warn_about_http_once ;;
    esac
    local status=0
    curl -fsSL --max-time 30 -o "$destination" "$uri" || status=$?
    if [ "$status" -eq 0 ]; then
        return $SOURCE_OK
    elif [ "$status" -eq "$CURL_HTTP_ERROR" ]; then
        return $SOURCE_UNRESOLVED
    else
        return $SOURCE_UNREACHABLE
    fi
}

# Resolve one relative name against the configured sources, in order.
#
# Prints three lines -- the URI it resolved, a local path holding that text, and where the text came
# from (`local`, `network` or `cache`) -- and exits 0, 1 (no source offers this name) or 2 (a source
# could not be reached and nothing of it is cached).
#
# The order of preference within one candidate is deliberate. Offline, the cache is the only
# answer. Online, the network is asked first and the cache is the fallback, rather than the other
# way round: a cache that answered first would quietly pin a methodology to whatever was fetched the
# day it was first seen, and the published text changing is precisely the event this mechanism
# exists to notice.
resolve_relative_name() {
    local relative="$1"
    require_sources || return 1

    local outcome=$SOURCE_UNRESOLVED
    local source uri path cached fetched

    for source in "${VALIDATE_SOURCES[@]}"; do
        uri=$(source_uri_for "$source" "$relative")

        path=$(local_path_of "$uri")
        if [ -n "$path" ]; then
            if [ -f "$path" ]; then
                printf '%s\n%s\n%s\n' "$uri" "$path" "local"
                return $SOURCE_OK
            fi
            continue
        fi

        cached=$(cached_body_for "$uri")

        if [ -n "$FREEPORTS_VALIDATE_OFFLINE" ]; then
            if [ -n "$cached" ]; then
                printf '%s\n%s\n%s\n' "$uri" "$cached" "cache"
                return $SOURCE_OK
            fi
            outcome=$SOURCE_UNREACHABLE
            continue
        fi

        fetched=$(mktemp "${TMPDIR:-/tmp}/freeports-validate.XXXXXX") || return 1
        local status=0
        fetch_uri "$uri" "$fetched" || status=$?
        if [ "$status" -eq "$SOURCE_OK" ]; then
            local stored
            stored=$(store_in_cache "$uri" "$fetched")
            rm -f "$fetched"
            printf '%s\n%s\n%s\n' "$uri" "$stored" "network"
            return $SOURCE_OK
        fi
        rm -f "$fetched"

        # Unreachable, but previously seen: the cache is a worse answer than a fresh fetch and a
        # better one than nothing, and the caller is told which it got so that it can say so.
        if [ "$status" -eq "$SOURCE_UNREACHABLE" ]; then
            if [ -n "$cached" ]; then
                printf '%s\n%s\n%s\n' "$uri" "$cached" "cache"
                return $SOURCE_OK
            fi
            outcome=$SOURCE_UNREACHABLE
        fi
    done

    return $outcome
}

resolve_methodology() {
    resolve_relative_name "$(methodology_relative_name "$1")"
}

resolve_general_methodology() {
    resolve_relative_name "$(general_methodology_relative_name)"
}

# The hash of a methodology's text: what a validation document records, and what every check
# compares against. Exits with the resolver's own status, so a caller can still tell an unresolved
# name from an unreachable source.
sha256_of_relative_name() {
    local resolution status=0
    resolution=$(resolve_relative_name "$1") || status=$?
    [ "$status" -eq 0 ] || return $status
    sha256_of_file "$(echo "$resolution" | sed -n 2p)"
}

methodology_sha256() {
    sha256_of_relative_name "$(methodology_relative_name "$1")"
}

general_methodology_sha256() {
    sha256_of_relative_name "$(general_methodology_relative_name)"
}
