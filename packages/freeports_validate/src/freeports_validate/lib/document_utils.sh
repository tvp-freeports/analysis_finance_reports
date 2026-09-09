#!/bin/bash

# Document processing utilities for validation documents

# Function to get array length from document
get_document_array_length() {
    local doc_path="$1"
    local array_path="$2"
    yq -r ".$array_path | length" "$doc_path" 2>/dev/null
}

# Function to get array element from document
get_document_array_element() {
    local doc_path="$1"
    local array_path="$2"
    local index="$3"
    local field="$4"
    yq -r ".$array_path[$index].$field" "$doc_path" 2>/dev/null
}

# Function to check if methodology exists in document
methodology_exists() {
    local doc_path="$1"
    local methodology_name="$2"
    yq -e ".methodologies[] | select(.name == \"$methodology_name\")" "$doc_path" > /dev/null 2>&1
}

# Function to check if file is granted with methodology
file_granted_with_methodology() {
    local doc_path="$1"
    local methodology_name="$2"
    local file_path="$3"
    yq -e ".data[] | select(.methodology == \"$methodology_name\") | .files[] | select(.path == \"$file_path\")" "$doc_path" > /dev/null 2>&1
}

# Function to add methodology to document
add_methodology_to_document() {
    local doc_path="$1"
    local methodology_name="$2"
    local methodology_hash="$3"
    
    yq -y -i ".methodologies += [{\"name\": \"$methodology_name\", \"sha256\": \"$methodology_hash\"}]"  "$doc_path"
}

# Every path already granted under a methodology, one per line.
#
# Asked once and kept, rather than asked again per file. `grant` needs to know whether each of the
# files it was handed is already there, and a document holding hundreds of them answers that
# question in a single read as easily as in one -- the difference is a `yq` per file over a
# document that grows as the loop runs, which is what made granting a whole test suite take
# minutes rather than seconds.
granted_paths_under_methodology() {
    local doc_path="$1"
    local methodology_name="$2"
    yq -r --arg methodology "$methodology_name" \
        '[.data[]? | select(.methodology == $methodology) | .files[]?.path] | .[]' \
        "$doc_path" 2>/dev/null
}

# Add many file grants in one write.
#
# The entries arrive as a JSON array in a *file*, not as an argument, and that is the point rather
# than a detail: a grant of several hundred files is tens of kilobytes of JSON, and an argument is
# capped at 128 KiB by the kernel however much room the whole command line has. `collect` was
# already losing to that limit; this is the same mistake not made twice.
#
# The methodology arrives through `--arg` for a smaller reason of the same kind: interpolating a
# name straight into the filter breaks on the first name that contains a quote.
add_file_grants_to_document() {
    local doc_path="$1"
    local methodology_name="$2"
    local entries_file="$3"

    # Create methodology section if it doesn't exist
    if ! yq -e --arg methodology "$methodology_name" \
        '.data[] | select(.methodology == $methodology)' "$doc_path" > /dev/null 2>&1; then
        yq -y -i --arg methodology "$methodology_name" \
            '.data += [{"methodology": $methodology, "files": []}]' "$doc_path"
    fi

    yq -y -i --slurpfile new "$entries_file" --arg methodology "$methodology_name" \
        '(.data[] | select(.methodology == $methodology).files) += $new[0]' "$doc_path"
}


# ---------------------------------------------------------------------------
# What used to be below this line
# ---------------------------------------------------------------------------
# A callback-driven iteration API -- `iterate_methodologies`, `iterate_data_sections`,
# `iterate_files_in_data_section`, `get_all_granted_files` and its two callbacks -- plus
# `get_document_field` and the singular `add_file_grant_to_document`. Eighty of this file's
# hundred and eighty-eight lines, and **nothing called any of them**: not a subcommand, not a
# library, not a test.
#
# They were removed rather than left alone because of what they contained. Each one read a document
# one field and one index at a time -- `iterate_files_in_data_section` spent two `yq` per file, and
# `yq` here is a Python program that costs about seventy-five milliseconds to start. That is the
# exact shape this library has twice been fixed *out* of: `granted_paths_under_methodology` above
# asks its question in one read, `add_file_grants_to_document` writes a whole batch in one, and
# `validation_utils.sh` reads each array with a single `mapfile`. Every one of those has a comment
# saying why.
#
# Dead code that demonstrates the pattern the live code was rewritten to avoid is not neutral. It is
# the copy somebody reaches for when they need to walk a document, and it would have put the
# quadratic behaviour back. If a callback-driven walk is ever wanted, write one that reads the
# arrays once -- there are three examples of how in this repository.
