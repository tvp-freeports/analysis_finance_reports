#!/bin/bash

# Validation utilities for consistent validation document processing

# Color codes
COLOR_RED='\033[0;31m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[1;33m'
COLOR_BLUE='\033[0;34m'
COLOR_RESET='\033[0m'

# Output functions
print_error()   { echo -e "${COLOR_RED}❌ $1${COLOR_RESET}" >&2; }
print_warning() { echo -e "${COLOR_YELLOW}⚠️  $1${COLOR_RESET}" >&2; }
print_success() { echo -e "${COLOR_GREEN}✅ $1${COLOR_RESET}"; }
print_info()    { echo -e "${COLOR_BLUE}ℹ️  $1${COLOR_RESET}"; }

# What a failure *means*, as opposed to what failed.
#
# The checks below each report one line, and those lines name a discrepancy: a hash that differs, a
# signature that does not verify. That is precise, and it is not an explanation -- a reader who did
# not build the mechanism cannot tell from "Invalid signature" whether somebody tampered with a
# document or whether they are simply missing a public key, and those two readings call for very
# different reactions. So each failure also records the *kind* of thing it is, and the run ends by
# spelling out, once, what each kind that actually occurred can mean.
#
# Recording happens only in the verbose branches, where the line is printed, which keeps the two in
# step: every paragraph at the end explains a line the reader saw above it, and no paragraph appears
# for a check made quietly on somebody's behalf by `who-grants` and its neighbours.
VALIDATION_DIAGNOSES=""

# A check that could not be made is neither a pass nor a failure, and the difference has to survive
# all the way to the exit status. A methodology page that cannot be fetched is the case that made
# this necessary: reporting it as a mismatch would accuse a granter of nothing, and reporting it as
# a match would vouch for a text nobody has seen.
#
# It is also the exit status of a whole run that ended that way -- see `bin/check-grants`. Three
# outcomes, three statuses: 0 the claims hold, 1 some of them do not, 3 some of them were not
# looked at.
readonly CHECK_INCONCLUSIVE=3

# Did this run leave anything unchecked?
#
# An unmade check is not an error, and it is not a pass either, and for a long time only the first
# half of that was implemented: the count of errors stayed at zero, so the run ended by announcing
# that every document had passed verification -- of grants it had in fact compared against nothing.
# That is the one reading a validation tool must never offer, because the whole value of a grant is
# that somebody checked it, and "I could not look" reported as "I looked and it was fine" destroys
# exactly that.
#
# So the two are tracked separately. The count decides between passing and failing; this flag says
# whether the question was fully asked, and a run where it was not says so and exits non-zero
# without calling anything wrong.
VALIDATION_UNVERIFIED=false

note_unverified() {
    VALIDATION_UNVERIFIED=true
}

# What each name actually resolved to during this run, so that the explanation at the end can show
# it. A hash mismatch is ambiguous by design -- the document records a name and a hash and
# deliberately not a source -- and the one thing that makes it diagnosable is being told which text
# *this* run compared against.
VALIDATION_RESOLVED_URIS=""

note_resolved_uri() {
    local line="    $1 -> $2"
    case "$VALIDATION_RESOLVED_URIS" in
        *"$line"*) return 0 ;;
    esac
    VALIDATION_RESOLVED_URIS="${VALIDATION_RESOLVED_URIS}${line}"$'\n'
}

note_diagnosis() {
    case " $VALIDATION_DIAGNOSES " in
        *" $1 "*) return 0 ;;
    esac
    VALIDATION_DIAGNOSES="$VALIDATION_DIAGNOSES $1"
}

# The heading is the failing line's own wording, so that the explanation is found by the reader who
# is looking at the red line rather than at a taxonomy they have never met.
diagnosis_title() {
    case "$1" in
        schema)                   echo "Document schema validation failed" ;;
        unsigned)                 echo "No signature found" ;;
        bad-signature)            echo "Invalid signature" ;;
        version)                  echo "Version mismatch" ;;
        methodology-unresolved)   echo "Methodology not offered by any configured source" ;;
        methodology-unreachable)  echo "Methodology could not be reached" ;;
        methodology-hash)         echo "Methodology hash mismatch" ;;
        path-unsupported)         echo "Granted outside the methodology's declared paths" ;;
        subhash-mismatch)         echo "A resource the methodology pins has changed" ;;
        subhash-unreachable)      echo "A resource the methodology pins could not be reached" ;;
        file-missing)             echo "File not found" ;;
        file-hash)                echo "File hash mismatch" ;;
    esac
}

# Written unexpanded, so that the backticks and the `$` of a shell example in the prose stay
# literal. The two values worth filling in are substituted by name where the text is printed.
diagnosis_meaning() {
    case "$1" in
        schema)
            cat <<'EOF'
The file is not shaped the way a validation document has to be, so nothing else
about it can be trusted to mean what it appears to. This is almost always a field
written by hand, and the one that catches everyone is `who.pubkey_id`: it must be
the full 40-digit fingerprint, not the 16-digit long key ID that
`gpg --list-keys --keyid-format=long` prints. To see which field the schema
objected to, run the check without swallowing its output:
    check-jsonschema --default-filetype yaml <document> \
        --schemafile $LIB_DIR/document.schema.json
EOF
            ;;
        unsigned)
            cat <<'EOF'
The document exists, and may be full of grants, but nobody has put their name to
it -- so it vouches for nothing. A grant is a claim by a person, and without a
signature there is no person behind it. A freshly created document stays unsigned
until `freeports-validate sign-document` is run on it.
EOF
            ;;
        bad-signature)
            cat <<'EOF'
The document does not verify against the key that signed it. Three quite different
situations produce this one message, and they are nowhere near equally likely:
  - The signer's public key is not in your keyring. This is by far the most common
    cause, and it says nothing whatever about the document: gpg cannot check what
    it cannot find, and fails the same way it would for a forgery. Import the key
    -- `gpg --import their.pub.asc`, or `gpg --locate-keys their@email` -- before
    concluding anything about the author.
  - The document changed after it was signed, by hand or by a tool that rewrote
    the YAML. The signature covers the document with its own `sign` field removed
    and every key sorted recursively, so even a reordering that leaves the meaning
    intact breaks it. Its author restates the claim with
    `freeports-validate sign-document --update`.
  - Last, and rarest: the `sign` block itself was truncated, mangled in transit,
    or replaced.
An expired key is not among the causes. A signature made while a key was valid
keeps verifying after that key lapses, so letting one expire never silently
invalidates grants already issued.
EOF
            ;;
        version)
            cat <<'EOF'
The document was written under a different text of the general methodology than
the one your sources resolve today. The `version` field pins that page by hash,
and the two readings are the same two as for a methodology below: either you and
the author of this document are configured with different sources, or the page
itself has been rewritten since. `freeports-validate sources` prints what each of
your sources resolves, which settles it.
The entries in the document mean what the pinned text says they mean, which is why
this is worth reading rather than clearing. `freeports-validate update version`
restates the document under the text you resolve now, and should be run by someone
who has looked at what changed.
EOF
            ;;
        methodology-unresolved)
            cat <<'EOF'
The document adopts a methodology that none of your configured sources offers, so
its claims cannot even be read, let alone checked. The pages do not ship with this
command: they are fetched from the sources you name, and a name that resolves
nowhere is almost always a source you have not added rather than a methodology
that has ceased to exist.
`freeports-validate sources` lists what each of your sources offers. If the
methodology is somebody else's, ask them which source they publish it at and add
that one; the sources you configure are exactly the set of authors you are willing
to read. If it really is gone, withdraw the methodology and everything granted
under it.
EOF
            ;;
        methodology-unreachable)
            cat <<'EOF'
This is not a failure and not a pass: the page could not be fetched, so nothing
was compared. A remote source was named, the network did not answer, and no copy
of that page had been cached from an earlier run. The grants under it are neither
confirmed nor called into question by this run -- they are simply unexamined, and
saying so is more honest than either alternative.
Re-run with a network, or point `validate.sources` at a checked-out copy of the
pages. `--offline` makes this the deliberate mode rather than an accident: it never
fetches, answers from the cache where it can, and reports this where it cannot.
EOF
            ;;
        methodology-hash)
            cat <<'EOF'
Two quite different things produce this, and the first is much the likelier:
  - You and the author of this document resolved the methodology from
    **different sources**. The document records a name and a hash and deliberately
    not a source, because the source is a contract between the granter and whoever
    verifies the grant -- each of you writes it in your own configuration. So a
    hash that differs may mean nothing more than that the two of you are reading
    two publications of the same methodology. The sources this run used, and what
    the name resolved to, are printed below; compare them with the author's.
  - The page itself has been rewritten since the grant was made. Every grant
    citing it was a claim about that text, so those claims no longer say what
    their author said. This is the mechanism working, not a bookkeeping nuisance.
`freeports-validate sources` shows what each configured source resolves, which is
how the two are told apart. Once you know which it is,
`freeports-validate update methodology "<name>"` restates the adoption under the
text you resolve now, and drops every file granted under it. That is deliberate:
the alternative is carrying old claims forward under a new meaning.
EOF
            ;;
        path-unsupported)
            cat <<'EOF'
This is a warning and never a failure. A methodology page may declare which
repository paths it applies to, and one of the grants below names a path outside
that set -- so the claim it makes is one the methodology does not, on its own
terms, say anything about.
There are three ordinary ways to arrive here, and none of them is an accusation:
  - The page gained a `Supported paths` section, or a stricter one, after the
    grant was signed. Its author widened or narrowed what the methodology means,
    and the grants made under the older text are what this line is pointing at.
  - The same path means different things in different repositories -- `tests/`
    in a formats repository is not `tests/` in the engine's own -- and the
    declaration was written with the other one in mind. The prose under each
    pattern is where that is meant to be settled; read it.
  - The grant really was made under the wrong methodology.
Deciding is the granter's, which is why this never fails a run: a repository must
not go red because somebody else edited a page it adopts. Withdraw the grant with
`freeports-validate ungrant` if it was a mistake, or leave it and say why. The
patterns the methodology does declare are printed below.
EOF
            ;;
        subhash-mismatch)
            cat <<'EOF'
A methodology page cites something -- a diagram, a specification, another
document -- and pins it by hash in a `.. sha256:` comment. One of those things no
longer hashes to what the page says. The page itself is intact: its own hash was
checked before this, and it matched, so what changed is a document the page
depends on rather than the methodology's own text.
That is worth knowing precisely because the page's hash cannot tell you. A grant
made under this methodology was, in part, a claim about what the cited document
said, and the cited document is now something else. Nobody's signature is wrong;
what a signature meant has moved underneath it.
Read what changed. If the methodology still means what it meant, its author
re-pins the page with `freeports-validate refresh-links`, which changes the
page's own hash and so asks everyone who adopted it to look. If it does not, the
methodology needs rewriting rather than re-pinning.
This is only ever reported under `--deep`. Pins are lines of the page, so the
page's hash already commits to them: checking them is a deepening, never a second
thing to trust, and a run that did not do it is not thereby less sure of anything.
EOF
            ;;
        subhash-unreachable)
            cat <<'EOF'
This is not a failure and not a pass. A resource the methodology page pins could
not be fetched, so nothing was compared, and the grants under that methodology are
neither confirmed nor called into question by this run.
Re-run with a network. `--offline` makes it deliberate: it never fetches, answers
from what has already been cached, and reports this where it cannot. A pinned
resource that is permanently gone is a different matter, and the methodology's
author is the one who can say what should replace it.
EOF
            ;;
        file-missing)
            cat <<'EOF'
A granted path is not in the repository at all. Either the file was moved or
deleted, in which case the grant is now about nothing and should be withdrawn with
`freeports-validate ungrant`; or this run is looking at the wrong tree, since
granted paths are stored relative to the repository root, and the root in use here
is
    $REPO_ROOT
Name another with --repo if that is not the tree the document is about.
EOF
            ;;
        file-hash)
            cat <<'EOF'
The file changed after it was granted. A grant is a claim about bytes and does not
follow them, which is the entire point: the signature stays attached to what was
actually reviewed. If the change was expected -- a format was fixed, and its
reference output genuinely should differ -- the person who confirmed that restates
the claim:
    freeports-validate update file <path> with "<methodology>"
If it was not expected, this line is the signal the mechanism exists to produce,
and restating the claim would convert that signal into a signature.
EOF
            ;;
    esac
}

# Printed once at the end of a run, covering every kind of failure that occurred in it -- not once
# per document, which would repeat the same paragraph for every contributor with the same stale hash.
print_diagnoses() {
    [ -z "$VALIDATION_DIAGNOSES" ] && return 0

    echo "" >&2
    echo -e "${COLOR_BLUE}ℹ️  What these errors mean${COLOR_RESET}" >&2
    echo "==========================================" >&2

    # A fixed order -- the order the checks run in -- rather than the order the failures happened to
    # occur in: what is being read here is a reference section, not a log.
    local kind
    for kind in schema unsigned bad-signature version methodology-unresolved \
                methodology-unreachable methodology-hash subhash-mismatch \
                subhash-unreachable path-unsupported file-missing file-hash; do
        case " $VALIDATION_DIAGNOSES " in
            *" $kind "*) ;;
            *) continue ;;
        esac
        echo "" >&2
        echo -e "${COLOR_YELLOW}$(diagnosis_title "$kind")${COLOR_RESET}" >&2
        diagnosis_meaning "$kind" \
            | sed -e "s|\$LIB_DIR|$LIB_DIR|g" -e "s|\$REPO_ROOT|$REPO_ROOT|g" -e 's|^|    |' >&2
        diagnosis_context "$kind" >&2
    done

    VALIDATION_DIAGNOSES=""
    VALIDATION_RESOLVED_URIS=""
    VALIDATION_UNSUPPORTED_METHODOLOGIES=""
}

# Which methodologies had a grant outside their declared paths, so the explanation can end by
# printing what each of them *does* declare. Newline-separated because a methodology's name has
# spaces in it.
VALIDATION_UNSUPPORTED_METHODOLOGIES=""

note_unsupported_methodology() {
    local line="$1"
    case $'\n'"$VALIDATION_UNSUPPORTED_METHODOLOGIES" in
        *$'\n'"$line"$'\n'*) return 0 ;;
    esac
    VALIDATION_UNSUPPORTED_METHODOLOGIES="${VALIDATION_UNSUPPORTED_METHODOLOGIES}${line}"$'\n'
}

# The part of an explanation that is about *this* run rather than about the kind of failure.
#
# Kept out of `diagnosis_meaning` on purpose: that function is written unexpanded so the backticks
# and shell examples in its prose stay literal, and it is a reference text that reads the same on
# every machine. What follows is the opposite -- the sources this particular run was configured
# with, and what this particular name resolved to -- and without it the three source-related
# failures cannot actually be diagnosed by the person reading them.
diagnosis_context() {
    case "$1" in
        version|methodology-unresolved|methodology-unreachable|methodology-hash)
            echo ""
            echo "    The sources this run used, in priority order:"
            printf '        %s\n' "${VALIDATE_SOURCES[@]}"
            if [ -n "$VALIDATION_RESOLVED_URIS" ]; then
                echo ""
                echo "    What each name resolved to here:"
                printf '%s' "$VALIDATION_RESOLVED_URIS" | sed 's|^    |        |'
            fi
            ;;
        path-unsupported)
            local methodology
            while IFS= read -r methodology; do
                [ -n "$methodology" ] || continue
                echo ""
                echo "    What \"$methodology\" declares it supports:"
                print_supported_paths "$methodology" | sed 's|^    |        |'
            done <<<"$VALIDATION_UNSUPPORTED_METHODOLOGIES"
            ;;
    esac
}

# Turn a resolver status into the line the reader sees and the paragraph they get at the end.
#
# One function rather than the same `case` at four call sites, because the distinction it draws --
# a name nothing offers, against a source nothing could reach -- is the distinction the whole
# resolver exists to preserve, and it must be drawn the same way everywhere.
report_methodology_resolution() {
    local status="$1" what="$2" verbose="${3:-false}"
    [ "$verbose" = "true" ] || return 0
    case "$status" in
        "$SOURCE_UNRESOLVED")
            print_error "$what: no configured source offers it"
            note_diagnosis methodology-unresolved
            ;;
        "$SOURCE_UNREACHABLE")
            print_warning "$what: could not be reached, so nothing was compared"
            note_diagnosis methodology-unreachable
            ;;
    esac
}

# How the text got here, said in the success line. "Matches" means something weaker when the copy
# compared against came out of a cache rather than off the network, and the reader is entitled to
# know which they were told.
origin_note() {
    case "$1" in
        cache) echo " (from the cache; nothing was fetched)" ;;
        *)     echo "" ;;
    esac
}

# Validate schema
validate_document_schema() {
    local doc_path="$1"
    local verbose="${2:-false}"

    if [ ! -f "$doc_path" ]; then
        [ "$verbose" = "true" ] && print_error "Document file not found: $doc_path"
        return 1
    fi
    if check-jsonschema --schemafile "${LIB_DIR}/document.schema.json" --default-filetype yaml "$doc_path" >/dev/null 2>&1; then
        [ "$verbose" = "true" ] && print_success "Document schema is valid"
        return 0
    else
        [ "$verbose" = "true" ] && { print_error "Document schema validation failed"; note_diagnosis schema; }
        return 1
    fi
}

# The exact bytes a signature is computed over: the document without its own `sign` field, with
# every key sorted recursively so that two writers of the same content produce the same stream.
#
# `-S` is jq's own recursive key sort. It is the whole reason this project standardises on the
# Python `yq`, which is a thin wrapper around jq: the other `yq` has its own expression language
# and would need `sortKeys(..)` here, and the two do not emit identical bytes.
canonical_document() {
    local doc_path="$1"
    yq -y -S 'del(.sign)' "$doc_path"
}

# Validate document signature
validate_document_signature() {
    local doc_path="$1"
    local verbose="${2:-false}"

    local existing_sig
    existing_sig=$(yq -r '.sign // ""' "$doc_path")

    if [ -z "$existing_sig" ]; then
        [ "$verbose" = "true" ] && { print_warning "No signature found"; note_diagnosis unsigned; }
        return 2
    fi

    if canonical_document "$doc_path" | gpg --verify <(echo "$existing_sig") - >/dev/null 2>&1; then
        [ "$verbose" = "true" ] && print_success "Signature is valid"
        return 0
    else
        [ "$verbose" = "true" ] && { print_error "Invalid signature"; note_diagnosis bad-signature; }
        return 1
    fi
}

# Generate signature
generate_document_signature() {
    local doc_path="$1"
    canonical_document "$doc_path" | gpg --detach-sign --armor --default-key "$KEYID" -
}

# Add signature to document
add_signature_to_document() {
    local doc_path="$1"
    local signature="$2"

    # The signature is written as a YAML *literal block* rather than left to the emitter, which is
    # the difference between an armored signature that reads like one and an armored signature with
    # a blank line between every pair of its own lines.
    #
    # Letting `yq` set the field produced the latter. Nothing was wrong with the data -- a
    # single-quoted YAML scalar folds a line break into a space, so a real newline has to be written
    # as a blank line, and reading it back gave the signature intact. It just made the one part of
    # the document a person might actually want to look at twice as tall and hard to compare by eye.
    # A literal block has no such encoding: every line stands as it is.
    #
    # Safe to restyle, because the signature is computed over the document with `.sign` deleted --
    # see `canonical_document`. How this field is written cannot affect what it attests to, and a
    # document signed before this change still verifies, since reading is unaffected by style.
    local body
    body=$(yq -y 'del(.sign)' "$doc_path") || return 1

    local tmp
    tmp=$(mktemp "${doc_path}.XXXXXX") || return 1

    {
        printf '%s\n' "$body"
        # `|-`, not `|`: the strip indicator drops the block's closing newline, so the string is
        # byte-identical to what the emitter used to store. The schema pins the signature to end at
        # `-----END PGP SIGNATURE-----`, and a clip block would append a newline past it.
        printf 'sign: |-\n'
        # Indented into the block, except for the armor's own blank lines: indenting those would
        # leave trailing whitespace on an otherwise empty line, which YAML reads the same way and
        # every linter complains about.
        printf '%s\n' "$signature" | awk '{ if (length($0)) print "  " $0; else print "" }'
    } > "$tmp" || { rm -f "$tmp"; return 1; }

    mv "$tmp" "$doc_path"
}

# Does the text this document was written under still hash to what the document says?
#
# Three outcomes, not two. The page is resolved from the configured sources, and a source that
# cannot be reached leaves the question *unanswered* -- which is neither a match nor a mismatch, and
# is reported as `CHECK_INCONCLUSIVE` so that it never turns a run red on its own.
validate_methodology_hash() {
    local doc_path="$1"
    local methodology_name="$2"
    local verbose="${3:-false}"

    local stored_hash
    stored_hash=$(yq -r ".methodologies[] | select(.name == \"$methodology_name\") | .sha256" "$doc_path")

    if [ -z "$stored_hash" ]; then
        [ "$verbose" = "true" ] && print_error "Methodology not found: $methodology_name"
        return 1
    fi

    local resolution status=0
    resolution=$(resolve_methodology "$methodology_name") || status=$?

    if [ "$status" -eq "$SOURCE_UNRESOLVED" ]; then
        report_methodology_resolution "$status" "Methodology \"$methodology_name\"" "$verbose"
        return 1
    fi
    if [ "$status" -ne 0 ]; then
        report_methodology_resolution "$status" "Methodology \"$methodology_name\"" "$verbose"
        return $CHECK_INCONCLUSIVE
    fi

    local uri path origin
    { read -r uri; read -r path; read -r origin; } <<<"$resolution"
    note_resolved_uri "$methodology_name" "$uri"

    local current_hash
    current_hash=$(sha256_of_file "$path")

    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "Methodology \"$methodology_name\" hash matches$(origin_note "$origin")"
        return 0
    else
        [ "$verbose" = "true" ] && { print_error "Methodology \"$methodology_name\" hash mismatch (resolved from $uri)"; note_diagnosis methodology-hash; }
        return 1
    fi
}

# Validate file hash
# The fourth argument is the file's hash, when the caller has already read it.
#
# A caller checking one file has no reason to pass it -- `sha256sum` on that file is the whole of
# what this function claims, and it stays a command the reader can retype. A caller checking several
# hundred does: reading them all in one `sha256sum` is the same work, and starting two processes per
# file to do it is not. What must not change either way is which hash is compared with which, so the
# argument is a value the caller has, never a promise the caller makes.
validate_file_hash() {
    local file_path="$1"
    local stored_hash="$2"
    local verbose="${3:-false}"
    local known_hash="${4:-}"

    if [ ! -f "$file_path" ]; then
        [ "$verbose" = "true" ] && { print_error "File not found: $file_path"; note_diagnosis file-missing; }
        return 1
    fi

    local current_hash="$known_hash"
    [ -n "$current_hash" ] || current_hash=$(sha256sum "$file_path" | awk '{print $1}')
    local file_relative=$(realpath --relative-to="$REPO_ROOT" "$file_path")
    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "File \"$file_relative\" hash matches"
        return 0
    else
        [ "$verbose" = "true" ] && { print_error "File \"$file_relative\" hash mismatch"; note_diagnosis file-hash; }
        return 1
    fi
}

# Validate version
validate_version() {
    local doc_path="$1"
    local verbose="${2:-false}"
    local stored_version
    stored_version=$(yq -r '.version // ""' "$doc_path")

    if [ -z "$stored_version" ]; then
        [ "$verbose" = "true" ] && { print_error "No version found"; note_diagnosis version; }
        return 1
    fi

    local resolution status=0
    resolution=$(resolve_general_methodology) || status=$?

    if [ "$status" -eq "$SOURCE_UNRESOLVED" ]; then
        report_methodology_resolution "$status" "The general methodology" "$verbose"
        return 1
    fi
    if [ "$status" -ne 0 ]; then
        report_methodology_resolution "$status" "The general methodology" "$verbose"
        return $CHECK_INCONCLUSIVE
    fi

    local uri path origin
    { read -r uri; read -r path; read -r origin; } <<<"$resolution"
    note_resolved_uri "general methodology" "$uri"

    local current_version
    current_version=$(sha256_of_file "$path")

    if [ "$stored_version" = "$current_version" ]; then
        [ "$verbose" = "true" ] && print_success "Version matches$(origin_note "$origin")"
        return 0
    else
        [ "$verbose" = "true" ] && { print_error "Version mismatch (general methodology resolved from $uri)"; note_diagnosis version; }
        return 1
    fi
}

# Check contributor
is_contributor_document() {
    local doc_path="$1"
    local identifier="$2"

    local name email pubkey
    name=$(yq -r '.who.name // ""' "$doc_path")
    email=$(yq -r '.who.email // ""' "$doc_path")
    pubkey=$(yq -r '.who.pubkey_id // ""' "$doc_path")

    [[ "$name" = "$identifier" || "$email" = "$identifier" || "$pubkey" = "$identifier" ]]
}

# Get contributor info
get_contributor_info() {
    local doc_path="$1"
    local name
    name=$(yq -r '.who.name // "Unknown"' "$doc_path")
    echo "$name"
}

# Normalize methodology name
normalize_methodology_name() {
    echo "$1" | tr '_' ' ' | tr '[:upper:]' '[:lower:]'
}

# `get_methodology_path` and `get_methodology_hash` used to live here, and pointed at the pages that
# shipped with the package. `resolve_methodology` and `methodology_sha256` in `lib/sources.sh`
# replace them: same questions, asked of the sources the user configured.

# Run a check and translate "could not be made" into success, **for the purpose of counting only**.
#
# Not counting it is what keeps a repository from going red because somebody ran the check on a
# train: nothing was compared, so nothing was found wrong, and calling it a failure would accuse a
# granter over a network. `--offline` makes that state deliberate rather than accidental, and a run
# in it still reports every hash it *could* compare.
#
# But not counting it is not the same as passing it, which is why the flag is raised here as well.
# The caller needs both facts -- how many checks failed, and whether every check was actually made
# -- and it has them only because this function records the second one on its way past.
count_unless_inconclusive() {
    local status=0
    "$@" || status=$?
    if [ "$status" -eq "$CHECK_INCONCLUSIVE" ]; then
        note_unverified
        return 0
    fi
    return $status
}

# Comprehensive validation
validate_document_comprehensive() {
    local doc_path="$1"
    local errors=0

    print_info "Checking validation document: $(basename "$doc_path")"
    echo "=========================================="

    [ ! -f "$doc_path" ] && { print_error "Document not found"; return 1; }

    validate_document_schema "$doc_path" "true" || ((errors++))
    validate_document_signature "$doc_path" "true" || ((errors++))

    # `count_unless_inconclusive` and not `|| ((errors++))`: a check that could not be made must not
    # colour the run. It has already said so on its own line, and it will be explained at the end.
    count_unless_inconclusive validate_version "$doc_path" "true" || ((errors++))

    local count method_name
    count=$(yq -r '.methodologies | length' "$doc_path")
    for ((i=0; i<count; i++)); do
        method_name=$(yq -r ".methodologies[$i].name // \"\"" "$doc_path")
        [ -n "$method_name" ] || continue

        count_unless_inconclusive validate_methodology_hash "$doc_path" "$method_name" "true" \
            || ((errors++))

        # Only when asked. Following a page's own citations costs a fetch each, and the page's hash
        # already commits to the lines that pin them -- so this adds the transitive question,
        # "do the things this page cites still say what its author read?", to a run that wanted it.
        if [ -n "$FREEPORTS_VALIDATE_DEEP" ]; then
            verify_methodology_links "$method_name" || ((errors++))
        fi
    done

    count=$(yq -r '.data | length' "$doc_path")
    for ((i=0; i<count; i++)); do
        local method
        method=$(yq -r ".data[$i].methodology // \"\"" "$doc_path")
        print_info "Methodology: $method"

        # The whole entry in two reads rather than two per file. Both questions below -- what the
        # files hash to now, and which of them the methodology declares -- are then asked once for
        # all of them. Asked file by file, a document vouching for a test suite spent minutes
        # starting `yq`, `sha256sum` and a Python interpreter several thousand times over, for
        # answers that come back in one call each.
        local entry_paths=() entry_hashes=()
        mapfile -t entry_paths < <(yq -r "[.data[$i].files[]?.path // \"\"] | .[]" "$doc_path")
        mapfile -t entry_hashes < <(yq -r "[.data[$i].files[]?.sha256 // \"\"] | .[]" "$doc_path")
        [ "${#entry_paths[@]}" -gt 0 ] || continue

        local present_relative=() present_absolute=() present_hashes=()
        local file_path index
        for file_path in "${entry_paths[@]}"; do
            if [ -f "$REPO_ROOT/$file_path" ]; then
                present_relative+=("$file_path")
                present_absolute+=("$REPO_ROOT/$file_path")
            fi
        done

        declare -A current_hash_of=()
        if [ "${#present_absolute[@]}" -gt 0 ]; then
            mapfile -t present_hashes < <(sha256sum -- "${present_absolute[@]}" | awk '{print $1}')
            for index in "${!present_relative[@]}"; do
                current_hash_of["${present_relative[$index]}"]="${present_hashes[$index]}"
            done
        fi

        declare -A out_of_scope=()
        local scope_status=0
        unsupported_paths "$method" "${entry_paths[@]}" || scope_status=$?
        if [ "$scope_status" -ne "$PATH_UNDECIDABLE" ]; then
            for file_path in ${UNSUPPORTED_PATHS[@]+"${UNSUPPORTED_PATHS[@]}"}; do
                out_of_scope["$file_path"]=1
            done
        fi

        for index in "${!entry_paths[@]}"; do
            file_path="${entry_paths[$index]}"
            ! validate_file_hash "$REPO_ROOT/$file_path" "${entry_hashes[$index]}" "true" \
                "${current_hash_of[$file_path]:-}" && ((errors++))

            # Deliberately not counted, and deliberately asked even when the hash above failed:
            # the two say different things about the same grant, and a reader deciding what to do
            # about a changed file is better off knowing it was out of scope to begin with.
            if [ "$scope_status" -eq "$PATH_UNDECIDABLE" ]; then
                report_granted_path "$method" "$file_path" "$PATH_UNDECIDABLE"
            elif [ -n "${out_of_scope[$file_path]:-}" ]; then
                report_granted_path "$method" "$file_path" "$PATH_UNSUPPORTED"
            else
                report_granted_path "$method" "$file_path" "$PATH_SUPPORTED"
            fi
        done
        unset current_hash_of out_of_scope
    done

    echo "=========================================="
    if [ $errors -eq 0 ]; then
        print_success "All checks passed for $(basename "$doc_path")"
        return 0
    else
        print_error "Found $errors error(s) in $(basename "$doc_path")"
        return 1
    fi
}
