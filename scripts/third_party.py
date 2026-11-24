"""Lê a saída do `cargo tree` (pacote|licença) e grava THIRD_PARTY.md e THIRD_PARTY-pt-br.md."""
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
    groups[lic or "?"].append(pkg.replace(" v", " "))
total = sum(len(v) for v in groups.values())

TEXT = {
    "en": {
        "file": "THIRD_PARTY.md",
        "switch": "**English** · [Português (Brasil)](THIRD_PARTY-pt-br.md)",
        "title": "Third-party licenses",
        "intro": [
            "Planejei is distributed under the [GPL-3.0-or-later](LICENSE). It is compiled together with",
            f"the libraries below ({total} Rust packages), all under licenses compatible with the GPL-3.0.",
            "The full text of each license ships with the source code of each package",
            "([crates.io](https://crates.io)).",
        ],
        "fonts": "Fonts embedded in the interface",
        "fonts_intro": "egui (the interface library) embeds these fonts in the executable:",
        "font_col": "| Font | License |",
        "hack": "MIT; parts from Bitstream Vera (Bitstream Vera License) and DejaVu (public domain)",
        "icons": "Emoji Icon Font (egui icons)",
        "packages": "Rust packages by license",
        "unknown": "no license declared",
    },
    "pt": {
        "file": "THIRD_PARTY-pt-br.md",
        "switch": "[English](THIRD_PARTY.md) · **Português (Brasil)**",
        "title": "Licenças de terceiros",
        "intro": [
            "O Planejei é distribuído sob a [GPL-3.0-or-later](LICENSE). Ele é compilado junto com as",
            f"bibliotecas abaixo ({total} pacotes Rust), todas com licenças compatíveis com a GPL-3.0.",
            "Os textos completos de cada licença acompanham o código-fonte de cada pacote",
            "([crates.io](https://crates.io)).",
        ],
        "fonts": "Fontes embutidas na interface",
        "fonts_intro": "O egui (biblioteca da interface) embute estas fontes no executável:",
        "font_col": "| Fonte | Licença |",
        "hack": "MIT; partes da Bitstream Vera (Bitstream Vera License) e da DejaVu (domínio público)",
        "icons": "Emoji Icon Font (ícones do egui)",
        "packages": "Pacotes Rust por licença",
        "unknown": "sem licença declarada",
    },
}

for t in TEXT.values():
    out = [f"# {t['title']}", "", t["switch"], "", *t["intro"], "", f"## {t['fonts']}", "",
           t["fonts_intro"], "", t["font_col"], "|---|---|",
           "| Ubuntu Light | [Ubuntu Font Licence 1.0](https://ubuntu.com/legal/font-licence) |",
           f"| Hack | {t['hack']} |",
           "| Noto Emoji | [SIL Open Font License 1.1](https://openfontlicense.org) |",
           f"| {t['icons']} | MIT |",
           "", f"## {t['packages']}", ""]
    for lic, pkgs in sorted(groups.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        out += [f"### {t['unknown'] if lic == '?' else lic} ({len(pkgs)})", "",
                ", ".join(f"`{p}`" for p in sorted(pkgs, key=str.lower)), ""]
    open(t["file"], "w", encoding="utf-8").write("\n".join(out))
    print(t["file"], "atualizado")
