#!/bin/bash

# Document processing utilities for validation documents

# Function to get document field safely
get_document_field() {
    local doc_path="$1"
    local field="$2"
    yq -r ".$field" "$doc_path" 2>/dev/null
}

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
    
    yq -i ".methodologies += [{\"name\": \"$methodology_name\", \"sha256\": \"$methodology_hash\"}]" -y "$doc_path"
}

# Function to add file grant to document
add_file_grant_to_document() {
    local doc_path="$1"
    local methodology_name="$2"
    local file_path="$3"
    local file_hash="$4"
    
    # Create methodology section if it doesn't exist
    if ! yq -e ".data[] | select(.methodology == \"$methodology_name\")" "$doc_path" > /dev/null 2>&1; then
        yq -i ".data += [{\"methodology\": \"$methodology_name\", \"files\": []}]" -y "$doc_path"
    fi
    
    # Add file to methodology's files list
    yq -i "(.data[] | select(.methodology == \"$methodology_name\").files) += [{\"path\": \"$file_path\", \"sha256\": \"$file_hash\"}]" -y "$doc_path"
}

# Function to iterate over methodologies in document
iterate_methodologies() {
    local doc_path="$1"
    local callback="$2"
    local methodology_count
    
    methodology_count=$(get_document_array_length "$doc_path" "methodologies")
    
    for ((i=0; i<methodology_count; i++)); do
        local method_name
        method_name=$(get_document_array_element "$doc_path" "methodologies" "$i" "name")
        
        if [ -n "$method_name" ] && [ "$method_name" != "null" ]; then
            $callback "$doc_path" "$method_name" "$i"
        fi
    done
}

# Function to iterate over data sections in document
iterate_data_sections() {
    local doc_path="$1"
    local callback="$2"
    local data_count
    
    data_count=$(get_document_array_length "$doc_path" "data")
    
    for ((i=0; i<data_count; i++)); do
        local methodology
        methodology=$(get_document_array_element "$doc_path" "data" "$i" "methodology")
        
        if [ -n "$methodology" ] && [ "$methodology" != "null" ]; then
            $callback "$doc_path" "$methodology" "$i"
        fi
    done
}

# Function to iterate over files in data section
iterate_files_in_data_section() {
    local doc_path="$1"
    local data_index="$2"
    local callback="$3"
    local files_count
    
    files_count=$(yq -r ".data[$data_index].files | length" "$doc_path" 2>/dev/null)
    
    for ((j=0; j<files_count; j++)); do
        local file_path
        file_path=$(yq -r ".data[$data_index].files[$j].path" "$doc_path" 2>/dev/null)
        local file_hash
        file_hash=$(yq -r ".data[$data_index].files[$j].sha256" "$doc_path" 2>/dev/null)
        
        if [ -n "$file_path" ] && [ "$file_path" != "null" ]; then
            $callback "$doc_path" "$file_path" "$file_hash" "$j"
        fi
    done
}

# Function to get all granted files from document
get_all_granted_files() {
    local doc_path="$1"
    local -n files_array="$2"
    
    iterate_data_sections "$doc_path" "process_data_section_for_files"
}

process_data_section_for_files() {
    local doc_path="$1"
    local methodology="$2"
    local data_index="$3"
    
    iterate_files_in_data_section "$doc_path" "$data_index" "add_file_to_array"
}

add_file_to_array() {
    local doc_path="$1"
    local file_path="$2"
    local file_hash="$3"
    local file_index="$4"
    
    files_array["$file_path"]="$file_hash"
}