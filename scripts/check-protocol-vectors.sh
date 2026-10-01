#!/bin/sh
set -eu

if [ -n "${FLORA_PROTOCOL_VECTORS:-}" ]; then
  cmp --silent protocol_vectors.json "$FLORA_PROTOCOL_VECTORS"
else
  vector_commit=f893be6f85a0366843497aca3e2ed1c1dc251784
  tmp_file=$(mktemp)
  trap 'rm -f "$tmp_file"' EXIT
  curl --fail --location --silent --show-error \
    "https://raw.githubusercontent.com/FLORA-Research/cloud/${vector_commit}/protocol_vectors.json" \
    --output "$tmp_file"
  cmp --silent protocol_vectors.json "$tmp_file"
fi
