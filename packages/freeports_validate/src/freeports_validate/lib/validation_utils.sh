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
        [ "$verbose" = "true" ] && print_error "Document schema validation failed"
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
        [ "$verbose" = "true" ] && print_warning "No signature found"
        return 2
    fi

    if canonical_document "$doc_path" | gpg --verify <(echo "$existing_sig") - >/dev/null 2>&1; then
        [ "$verbose" = "true" ] && print_success "Signature is valid"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Invalid signature"
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
    # Passed through the environment rather than interpolated into the filter: an armored
    # signature is multi-line and full of characters a shell-quoted jq expression would mangle.
    export SIG_CONTENT="$signature"
    yq -y -i '.sign = env.SIG_CONTENT' "$doc_path"
    # Unset env variable after use
    unset SIG_CONTENT
}

# Validate methodology hash
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

    local method_file
    method_file=$(echo "$methodology_name" | tr ' ' '_' | tr '[:upper:]' '[:lower:]')
    local method_path="$METHODOLOGY_DIR/methodologies/${method_file}.rst"

    if [ ! -f "$method_path" ]; then
        [ "$verbose" = "true" ] && print_error "Methodology file not found: $method_path"
        return 1
    fi

    local current_hash
    current_hash=$(sha256sum "$method_path" | awk '{print $1}')

    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "Methodology \"$methodology_name\" hash matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Methodology \"$methodology_name\" hash mismatch"
        return 1
    fi
}

# Validate file hash
validate_file_hash() {
    local file_path="$1"
    local stored_hash="$2"
    local verbose="${3:-false}"

    if [ ! -f "$file_path" ]; then
        [ "$verbose" = "true" ] && print_error "File not found: $file_path"
        return 1
    fi

    local current_hash
    current_hash=$(sha256sum "$file_path" | awk '{print $1}')
    local file_relative=$(realpath --relative-to="$REPO_ROOT" "$file_path")
    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "File \"$file_relative\" hash matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "File \"$file_relative\" hash mismatch"
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
        [ "$verbose" = "true" ] && print_error "No version found"
        return 1
    fi

    local current_version
    current_version=$(sha256sum "$METHODOLOGY_DIR/general_methodology.rst" | awk '{print $1}')

    if [ "$stored_version" = "$current_version" ]; then
        [ "$verbose" = "true" ] && print_success "Version matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Version mismatch"
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

# Methodology path & hash helpers
get_methodology_path() {
    local name="$1"
    echo "$METHODOLOGY_DIR/methodologies/$(echo "$name" | tr ' ' '_' | tr '[:upper:]' '[:lower:]').rst"
}

get_methodology_hash() {
    local path
    path=$(get_methodology_path "$1")
    [ -f "$path" ] && sha256sum "$path" | awk '{print $1}'
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
    validate_version "$doc_path" "true" || ((errors++))

    local count method_name
    count=$(yq -r '.methodologies | length' "$doc_path")
    for ((i=0; i<count; i++)); do
        method_name=$(yq -r ".methodologies[$i].name // \"\"" "$doc_path")
        [ -n "$method_name" ] && ! validate_methodology_hash "$doc_path" "$method_name" "true" && ((errors++))
    done

    count=$(yq -r '.data | length' "$doc_path")
    for ((i=0; i<count; i++)); do
        local method
        method=$(yq -r ".data[$i].methodology // \"\"" "$doc_path")
        
        local file_count
        file_count=$(yq -r ".data[$i].files | length" "$doc_path")
        print_info "Methodology: $method"

        for ((j=0; j<file_count; j++)); do
            local file_path doc_hash full_path
            file_path=$(yq -r ".data[$i].files[$j].path" "$doc_path")
            doc_hash=$(yq -r ".data[$i].files[$j].sha256" "$doc_path")
            full_path="$REPO_ROOT/$file_path"
            ! validate_file_hash "$full_path" "$doc_hash" "true" && ((errors++))
        done
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
