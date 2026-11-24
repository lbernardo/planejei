#!/usr/bin/env bash
# Gera THIRD_PARTY.md (inglês) e THIRD_PARTY-pt-br.md com as licenças de todas as dependências
# que entram no programa (macOS e Windows). Rode de novo sempre que mudar o Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo tree -e normal --target all --prefix none -f "{p}|{l}" \
  | sed 's/ (\*)//; s/ (proc-macro)//' | sort -u | python3 scripts/third_party.py
