#!/usr/bin/env bash
# Gera o THIRD_PARTY.md com as licenças de todas as dependências que entram no programa
# (macOS e Windows). Rode de novo sempre que mudar o Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo tree -e normal --target all --prefix none -f "{p}|{l}" \
  | sed 's/ (\*)//; s/ (proc-macro)//' | sort -u | python3 -c '
import sys, collections
groups = collections.defaultdict(list)
for line in sys.stdin:
    pkg, _, lic = line.strip().partition("|")
    name = pkg.split(" ")[0]
    if not name or name == "planejei":
        continue
    lic = lic.replace("/", " OR ")
    # "MIT OR Apache-2.0" e "Apache-2.0 OR MIT" são a mesma escolha: ordena as opções.
    if "(" not in lic and " AND " not in lic and " WITH " not in lic:
        lic = " OR ".join(sorted(lic.split(" OR "), key=lambda x: ["MIT", "Apache-2.0"].index(x) if x in ("MIT", "Apache-2.0") else 9))
    groups[lic or "sem licença declarada"].append(pkg.replace(" v", " "))
total = sum(len(v) for v in groups.values())
print("# Licenças de terceiros\n")
print("O Planejei é distribuído sob a [GPL-3.0-or-later](LICENSE). Ele é compilado junto com as")
print(f"bibliotecas abaixo ({total} pacotes Rust), todas com licenças compatíveis com a GPL-3.0.")
print("Os textos completos de cada licença acompanham o código-fonte de cada pacote")
print("([crates.io](https://crates.io)).\n")
print("## Fontes embutidas na interface\n")
print("O egui (biblioteca da interface) embute estas fontes no executável:\n")
print("| Fonte | Licença |")
print("|---|---|")
print("| Ubuntu Light | [Ubuntu Font Licence 1.0](https://ubuntu.com/legal/font-licence) |")
print("| Hack | MIT; partes da Bitstream Vera (Bitstream Vera License) e da DejaVu (domínio público) |")
print("| Noto Emoji | [SIL Open Font License 1.1](https://openfontlicense.org) |")
print("| Emoji Icon Font (ícones do egui) | MIT |")
print("\n## Pacotes Rust por licença\n")
for lic, pkgs in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
    print(f"### {lic} ({len(pkgs)})\n")
    print(", ".join(f"`{p}`" for p in sorted(pkgs, key=str.lower)))
    print()
' > THIRD_PARTY.md
echo "THIRD_PARTY.md atualizado"
