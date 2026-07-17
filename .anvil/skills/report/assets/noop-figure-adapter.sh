#!/bin/sh
# noop-figure-adapter.sh — anvil's reference figure adapter (issue #427).
#
# This is the executable spec for the report figure-adapter contract
# (anvil/skills/report/commands/report-figure-adapter.md) and the test
# fixture for anvil/skills/report/lib/figure_adapters.py. It is NOT an
# EDA tool: it ignores the input's content and writes a minimal valid
# placeholder of the kind implied by the output path's extension.
# Anvil ships the contract, not the tooling — consumers replace this
# with their real generator (SPICE -> SVG, GDS -> PNG, ...).
#
# Usage:
#   noop-figure-adapter.sh <input> <output>
#
# Registered in .anvil/config.json as e.g.:
#   {
#     "version": 1,
#     "report": {
#       "figure_adapters": [
#         {
#           "name": "noop",
#           "command": "sh anvil/skills/report/assets/noop-figure-adapter.sh {input} {output}",
#           "input_glob": "src/*/schematic.sp",
#           "output_kind": "svg"
#         }
#       ]
#     }
#   }
#
# Contract behaviors demonstrated:
#   - exit 0 + non-empty, format-valid output  = success
#   - exit 2 on missing/unreadable input       = per-unit failure
#     (the dispatcher writes a <output>.FAILED.md stub and continues)
#   - POSIX sh + printf only — no deps beyond a shell.

set -u

if [ "$#" -ne 2 ]; then
    echo "usage: noop-figure-adapter.sh <input> <output>" >&2
    exit 2
fi

input=$1
output=$2

if [ ! -r "$input" ]; then
    echo "noop-figure-adapter: input not readable: $input" >&2
    exit 2
fi

case "$output" in
    *.svg)
        # Minimal valid SVG: XML declaration + 1x1 svg root with a label.
        printf '%s\n' \
            '<?xml version="1.0" encoding="UTF-8"?>' \
            '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="80">' \
            '  <rect width="320" height="80" fill="#eeeeee"/>' \
            '  <text x="10" y="45" font-family="sans-serif" font-size="14">noop-figure-adapter placeholder</text>' \
            '</svg>' > "$output"
        ;;
    *.png)
        # Minimal valid 1x1 transparent PNG (67 bytes), emitted as octal
        # escapes so this stays POSIX-printf-portable (no base64 flag
        # divergence between GNU and BSD).
        printf '\211PNG\r\n\032\n\000\000\000\015IHDR\000\000\000\001\000\000\000\001\010\006\000\000\000\037\025\304\211\000\000\000\012IDATx\234c\000\001\000\000\005\000\001\015\012-\264\000\000\000\000IEND\256B`\202' > "$output"
        ;;
    *.pdf)
        # Minimal single-blank-page PDF. Passes the %PDF magic-byte
        # check and opens in standard viewers.
        printf '%s\n' \
            '%PDF-1.4' \
            '1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj' \
            '2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj' \
            '3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >> endobj' \
            'trailer << /Root 1 0 R >>' \
            '%%EOF' > "$output"
        ;;
    *)
        echo "noop-figure-adapter: unsupported output extension: $output (expected .svg, .png, or .pdf)" >&2
        exit 2
        ;;
esac

exit 0
