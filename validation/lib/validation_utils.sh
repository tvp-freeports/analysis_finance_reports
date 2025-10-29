#!/bin/bash

# Validation utilities for consistent validation document processing

# Color codes for consistent output
export COLOR_RED='\033[0;31m'
export COLOR_GREEN='\033[0;32m'
export COLOR_YELLOW='\033[1;33m'
export COLOR_BLUE='\033[0;34m'
export COLOR_RESET='\033[0m'

# Output functions
print_error() {
    echo -e "${COLOR_RED}❌ $1${COLOR_RESET}" >&2
}

print_warning() {
    echo -e "${COLOR_YELLOW}⚠️  $1${COLOR_RESET}" >&2
}

print_success() {
    echo -e "${COLOR_GREEN}✅ $1${COLOR_RESET}"
}

print_info() {
    echo -e "${COLOR_BLUE}ℹ️  $1${COLOR_RESET}"
}

# Function to validate document signature
validate_document_signature() {
    local doc_path="$1"
    local verbose="${2:-false}"
    
    # Check if signature exists
    local existing_sig
    existing_sig=$(yq -r '.sign' "$doc_path" 2>/dev/null)
    if [ "$existing_sig" = "null" ] || [ -z "$existing_sig" ]; then
        [ "$verbose" = "true" ] && print_warning "No signature found"
        return 2
    fi
    
    # Verify the signature
    if yq -y -S 'del(.sign)' "$doc_path" 2>/dev/null | gpg --verify <(echo "$existing_sig" | base64 -d 2>/dev/null) - >/dev/null 2>&1; then
        [ "$verbose" = "true" ] && print_success "Signature is valid"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Invalid signature"
        return 1
    fi
}

# Function to check methodology hash validity
validate_methodology_hash() {
    local doc_path="$1"
    local methodology_name="$2"
    local verbose="${3:-false}"
    
    # Get stored hash from document
    local stored_hash
    stored_hash=$(yq -r ".methodologies[] | select(.name == \"$methodology_name\") | .sha256" "$doc_path" 2>/dev/null)
    
    if [ -z "$stored_hash" ] || [ "$stored_hash" = "null" ]; then
        [ "$verbose" = "true" ] && print_error "Methodology not found in document"
        return 1
    fi
    
    # Get methodology file path and calculate current hash
    local method_file
    method_file=$(echo "$methodology_name" | tr ' ' '_' | tr '[:upper:]' '[:lower:]')
    local method_path="$REPO_ROOT/docs/source/validation/methodologies/${method_file}.rst"
    
    if [ ! -f "$method_path" ]; then
        [ "$verbose" = "true" ] && print_error "Methodology file not found: $method_path"
        return 1
    fi
    
    local current_hash
    current_hash=$(sha256sum "$method_path" | awk '{print $1}')
    
    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "Methodology hash matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Methodology hash mismatch"
        return 1
    fi
}

# Function to check file hash validity
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
    
    if [ "$stored_hash" = "$current_hash" ]; then
        [ "$verbose" = "true" ] && print_success "File hash matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "File hash mismatch"
        return 1
    fi
}

# Function to check version validity
validate_version() {
    local doc_path="$1"
    local verbose="${2:-false}"
    
    # Get stored version from document
    local stored_version
    stored_version=$(yq -r '.version' "$doc_path" 2>/dev/null)
    
    if [ -z "$stored_version" ] || [ "$stored_version" = "null" ]; then
        [ "$verbose" = "true" ] && print_error "No version found in document"
        return 1
    fi
    
    # Calculate current version hash
    local current_version
    current_version=$(sha256sum "$REPO_ROOT/docs/source/validation/general_methodology.rst" | awk '{print $1}')
    
    if [ "$stored_version" = "$current_version" ]; then
        [ "$verbose" = "true" ] && print_success "Version matches"
        return 0
    else
        [ "$verbose" = "true" ] && print_error "Version mismatch"
        return 1
    fi
}

# Function to check if document belongs to contributor
is_contributor_document() {
    local doc_path="$1"
    local identifier="$2"
    
    # Check name
    local name
    name=$(yq -r '.who.name' "$doc_path" 2>/dev/null)
    if [ "$name" = "$identifier" ]; then
        return 0
    fi
    
    # Check email
    local email
    email=$(yq -r '.who.email' "$doc_path" 2>/dev/null)
    if [ "$email" = "$identifier" ]; then
        return 0
    fi
    
    # Check pubkey_id
    local pubkey_id
    pubkey_id=$(yq -r '.who.pubkey_id' "$doc_path" 2>/dev/null)
    if [ "$pubkey_id" = "$identifier" ]; then
        return 0
    fi
    
    return 1
}

# Function to get contributor info from document
get_contributor_info() {
    local doc_path="$1"
    local name=$(yq -r '.who.name' "$doc_path" 2>/dev/null)
    local email=$(yq -r '.who.email' "$doc_path" 2>/dev/null)
    local pubkey_id=$(yq -r '.who.pubkey_id' "$doc_path" 2>/dev/null)
    
    if [ -z "$name" ] || [ "$name" = "null" ]; then
        name="Unknown"
    fi
    
    echo "$name"
}

# Function to normalize methodology name
normalize_methodology_name() {
    echo "$1" | tr '_' ' ' | tr '[:upper:]' '[:lower:]'
}

# Function to get methodology file path
get_methodology_path() {
    local methodology_name="$1"
    local method_file=$(echo "$methodology_name" | tr ' ' '_' | tr '[:upper:]' '[:lower:]')
    echo "$REPO_ROOT/docs/source/validation/methodologies/${method_file}.rst"
}

# Function to get methodology hash
get_methodology_hash() {
    local methodology_name="$1"
    local method_path=$(get_methodology_path "$methodology_name")
    
    if [ ! -f "$method_path" ]; then
        return 1
    fi
    
    sha256sum "$method_path" | awk '{print $1}'
}

# Function to validate all aspects of a document
validate_document_comprehensive() {
    local doc_path="$1"
    local errors=0
    
    print_info "Checking validation document: $(basename "$doc_path")"
    echo "=========================================="
    
    # Check if document exists
    if [ ! -f "$doc_path" ]; then
        print_error "Document '$doc_path' does not exist"
        return 1
    fi
    
    # Validate signature
    if ! validate_document_signature "$doc_path" "true"; then
        if [ $? -eq 1 ]; then
            ((errors++))
        fi
    fi
    
    # Validate version
    if ! validate_version "$doc_path" "true"; then
        ((errors++))
    fi
    
    # Validate methodologies
    local methodology_count
    methodology_count=$(yq -r '.methodologies | length' "$doc_path" 2>/dev/null)
    
    for ((i=0; i<methodology_count; i++)); do
        local method_name
        method_name=$(yq -r ".methodologies[$i].name" "$doc_path" 2>/dev/null)
        
        if [ -n "$method_name" ] && [ "$method_name" != "null" ]; then
            if ! validate_methodology_hash "$doc_path" "$method_name" "true"; then
                ((errors++))
            fi
        fi
    done
    
    # Validate granted files
    local data_count
    data_count=$(yq -r '.data | length' "$doc_path" 2>/dev/null)
    
    for ((i=0; i<data_count; i++)); do
        local method_name
        method_name=$(yq -r ".data[$i].methodology" "$doc_path" 2>/dev/null)
        local files_count
        files_count=$(yq -r ".data[$i].files | length" "$doc_path" 2>/dev/null)
        
        print_info "Methodology: $method_name"
        
        for ((j=0; j<files_count; j++)); do
            local file_path
            file_path=$(yq -r ".data[$i].files[$j].path" "$doc_path" 2>/dev/null)
            local doc_hash
            doc_hash=$(yq -r ".data[$i].files[$j].sha256" "$doc_path" 2>/dev/null)
            
            local full_path="$REPO_ROOT/$file_path"
            
            if ! validate_file_hash "$full_path" "$doc_hash" "true"; then
                ((errors++))
            fi
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