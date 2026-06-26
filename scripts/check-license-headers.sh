#!/usr/bin/env bash
set -euo pipefail

expected_copyright='// Copyright (c) 2026 NicDevTV'
expected_license='// SPDX-License-Identifier: MIT'
missing=0

while IFS= read -r file; do
  first_line="$(sed -n '1p' "$file")"
  second_line="$(sed -n '2p' "$file")"

  if [[ "$first_line" != "$expected_copyright" || "$second_line" != "$expected_license" ]]; then
    echo "Missing license header: $file"
    missing=1
  fi
done < <(find . -type f -name '*.rs' \
  -not -path './target/*' \
  -not -path './.git/*' \
  | sort)

if [[ "$missing" -ne 0 ]]; then
  echo
  echo "Every Rust source file must start with:"
  echo "$expected_copyright"
  echo "$expected_license"
  exit 1
fi
