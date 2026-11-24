# Planejei

[English](README.md) · **Português (Brasil)**

![Cozinha de exemplo no Planejei](docs/video/composicao/img/s02.png)

Projeto de móveis planejados (marcenaria) inspirado no fluxo do Promob, irmão do GeoArq:
o mesmo motor 3D (Rust + wgpu + egui) e a mesma navegação. Você monta o ambiente com módulos
paramétricos e sai com **lista de peças, plano de corte e orçamento**.

Escopo e roadmap: [docs/SCOPE-pt-br.md](docs/SCOPE-pt-br.md). Vídeo de apresentação: [docs/video](docs/video/SCRIPT-pt-br.md).

## Rodar

```bash
cargo run --release                          # projeto vazio
PLANEJEI_DEMO=kitchen cargo run --release    # cozinha de exemplo
cargo run --release -- projeto.planejei      # abre um projeto
cargo test                                   # módulos, corte, orçamento, encaixe
```

## Gerar os instaláveis

```bash
./scripts/build.sh            # mac + windows
./scripts/build.sh mac        # só macOS (universal: Apple Silicon + Intel)
./scripts/build.sh windows    # só Windows (64 bits)
```

Roda os testes e deixa em `dist/`: `Planejei-<versão>-mac.zip` e `.dmg` (app assinado localmente),
`Planejei-<versão>-windows.zip` (portátil) e `Planejei-<versão>-setup.exe` (instalador, associa os
arquivos `.planejei`). Para o Windows, instale uma vez `brew install mingw-w64 makensis`.
A versão vem do `Cargo.toml`.

## Interface

- **Galeria** (esquerda): Cozinha, Dormitório, Banheiro e Escritório. Clicar insere o módulo
  encostado na parede do fundo, ao lado do último da mesma faixa (inferiores ou aéreos).
- **Propriedades** (direita): L × A × P, posição, rotação, portas, gavetas, prateleiras,
  divisórias, rodapé, cabideiro e materiais (caixa, frentes, fundo). Tudo recalcula na hora.
  Sem nada selecionado, edita o projeto e o ambiente.
- **Abas**: *3D*, *Plano de corte* (chapas desenhadas, aproveitamento) e *Orçamento*.
- **Menus**: Arquivo, Editar, Visualizar, Ambiente, Inserir, Ferramentas (materiais, ferragens,
  preços, serra e refilo), Relatórios e Ajuda.

| Ação | Como |
|---|---|
| Mover módulo | arrastar (encaixa e gira para a parede mais próxima e alinha com os vizinhos) |
| Orbitar / pan / zoom | arrastar o fundo ou o botão do meio / Shift + arrastar / roda ou pinça |
| Girar · Duplicar · Excluir | R · Cmd/Ctrl+D · Delete |
| Ajuste fino | setas (10 mm), Shift + setas (100 mm) |
| Vistas | 0 Iso · 1 Topo · 2 Frente · 3 Fundos · 4 Esquerda · 5 Direita · Shift+Z extensões |
| Ocultar portas e frentes | F |

As paredes que ficam entre a câmera e o ambiente somem sozinhas.

## Exportações

- **Lista de peças (CSV)**, separada por `;`, para otimizadores e seccionadoras
- **Plano de corte (SVG)**
- **Proposta (HTML)**: módulos, orçamento, lista de peças e plano de corte. Abre no navegador;
  Imprimir › Salvar como PDF.

## Ambiente do GeoArq

**Arquivo › Importar ambiente do GeoArq…** carrega a geometria de um `.geoarq` como referência
(o caminho fica salvo no projeto e o ambiente é recarregado ao abrir).

## Código

| Arquivo | O quê |
|---|---|
| `module.rs` | módulos paramétricos → peças (corte, fita de borda) e ferragens; galeria |
| `cutting.rs` | plano de corte em guilhotina (serra, refilo, veio) |
| `budget.rs` | orçamento (chapas, fita, ferragens, mão de obra, margem) |
| `project.rs` | projeto, ambiente, encaixe, `.planejei` e leitura do `.geoarq` |
| `reports.rs` | CSV, SVG e HTML |
| `app.rs` | interface |
| `renderer.rs`, `camera.rs`, `geom/`, `textures.rs`, `shader.wgsl` | motor copiado do GeoArq |

### Variáveis de desenvolvimento

Para conferir visualmente, `PLANEJEI_SCREENSHOT=saida.png cargo run` salva uma captura e fecha.
Combina com:

| Variável | Efeito |
|---|---|
| `PLANEJEI_DEMO=kitchen` | abre a cozinha de exemplo |
| `PLANEJEI_TAB=cut` ou `budget` | abre na aba Plano de corte ou Orçamento |
| `PLANEJEI_SELECT=2` | seleciona o módulo de índice 2 |
| `PLANEJEI_VIEW=top`, `front`… | vista inicial da câmera |
| `PLANEJEI_HIDE_FRONTS=1` | oculta portas e frentes |
| `PLANEJEI_CATALOG=1`, `PLANEJEI_PARTS=1`, `PLANEJEI_ABOUT=1` | abre a janela de materiais, de peças ou o "Sobre" |
| `PLANEJEI_SIZE=1600x900` | tamanho da janela |
| `PLANEJEI_DUMP=pasta` | grava o projeto e as exportações (CSV, SVG, HTML) nessa pasta |

O ícone é desenhado em `assets/icon.svg`; `icon.png`, `icon.icns` e `icon.ico` são gerados a
partir dele.

## Licença

Copyright (C) 2026 Lucas Bernardo.

O Planejei é software livre: você pode redistribuí-lo e modificá-lo nos termos da
[GNU General Public License](LICENSE), versão 3 ou (a seu critério) qualquer versão posterior
(`GPL-3.0-or-later`). Ele é distribuído sem nenhuma garantia.

As bibliotecas e fontes de terceiros estão em [THIRD_PARTY-pt-br.md](THIRD_PARTY-pt-br.md), gerado por
`./scripts/third_party.sh` (o `build.sh` roda esse script sozinho). Quem distribuir os
instaláveis também precisa oferecer o código-fonte correspondente, como manda a GPL.
