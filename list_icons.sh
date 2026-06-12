#!/bin/bash

# Define the root directories to search
SEARCH_DIRS=(
    "/usr/share/icons"
    "$HOME/.local/share/icons"
    "$HOME/.local/share/flatpak/appstream/flathub/x86_64/724b0f962e2504fe47cd53d7fce5085e518ca2fed6e66dd4444293d6b93f277d/icons"
    "/usr/share/pixmaps"
    "$HOME/.local/share/pixmaps"
)

# Output file
OUTPUT_FILE="icon_list.txt"

# Clear previous file
> "$OUTPUT_FILE"

# Counter for items
TOTAL_COUNT=0

# Print header to file
printf "%-30s | %s\n" "Filename" "Full Path" >> "$OUTPUT_FILE"
echo "--------------------------------------------------------------------------" >> "$OUTPUT_FILE"

# Loop through each directory
for dir in "${SEARCH_DIRS[@]}"; do
    if [ -d "$dir" ]; then
        # Search recursively for .png and .svg
        # -printf "%f\t%p\n" outputs the filename and the full path separated by a tab
        while IFS=$'\t' read -r filename filepath; do
            printf "%-30s | %s\n" "$filename" "$filepath" >> "$OUTPUT_FILE"
            ((TOTAL_COUNT++))
        done < <(find "$dir" -type f \( -name "*.png" -o -name "*.svg" \) -printf "%f\t%p\n")
    fi
done

# Print final total to file
echo "--------------------------------------------------------------------------" >> "$OUTPUT_FILE"
echo "Total icons (.png and .svg) found: $TOTAL_COUNT" >> "$OUTPUT_FILE"

echo "Results saved to $OUTPUT_FILE"
