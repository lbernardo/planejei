# Planejei — escopo

[English](SCOPE.md) · **Português (Brasil)**

Projeto de **móveis planejados** (marcenaria) inspirado no fluxo do Promob, irmão do
[GeoArq](../../sketchuo): mesmo motor (Rust + wgpu + egui), mesma navegação e mesmos atalhos
de câmera. O GeoArq desenha o ambiente; o Planejei mobilia, detalha e **orça**.

## Para quem

- **Marcenarias e lojas de planejados**: precisam de plano de corte, lista de peças e
  orçamento confiáveis, rápido, para fechar a venda e mandar para produção.
- **Arquitetos e designers de interiores**: querem especificar a marcenaria já com
  medidas reais e mostrar ao cliente.

## O que diferencia do Promob

| Promob | Planejei |
|---|---|
| Caro, licença por módulo (render, corte, orçamento) | Tudo incluso |
| Praticamente só Windows, pesado | Nativo, leve, Mac **e** Windows |
| Preso ao catálogo da fábrica | Biblioteca paramétrica própria, preços e chapas editáveis |
| Ambiente desenhado dentro dele | Ambiente simples embutido **ou** importado do GeoArq (`.geoarq`) |

## Interface (inspirada no Promob)

```
┌ Arquivo  Editar  Visualizar  Ambiente  Inserir  Ferramentas  Relatórios  Ajuda ┐
├ barra de ferramentas: Selecionar · Mover · Girar · Duplicar · Excluir │ vistas │ 3D/Corte/Orçamento
├──────────────┬──────────────────────────────────────────┬──────────────────────┤
│ Galeria      │                                          │ Propriedades         │
│ ▸ Cozinha    │            viewport 3D                    │  Dimensões (L×A×P)   │
│   Inferiores │     (ou plano de corte / orçamento)       │  Portas / gavetas    │
│   Aéreos     │                                          │  Prateleiras         │
│   Torres     │                                          │  Materiais (caixa,   │
│ ▸ Dormitório │                                          │  frente, fundo)      │
│ ▸ Banheiro   │                                          │  Posição / rotação   │
│ ▸ Escritório │                                          │  Peças do módulo     │
├──────────────┴──────────────────────────────────────────┴──────────────────────┤
│ status: módulo selecionado · nº de peças · chapas · total do orçamento          │
└────────────────────────────────────────────────────────────────────────────────┘
```

- **Galeria** (esquerda): árvore de categorias; clicar insere o módulo encaixado na parede.
- **Propriedades** (direita): parâmetros do módulo selecionado; tudo recalcula na hora.
- **Abas centrais**: *3D*, *Plano de corte*, *Orçamento*.

## Modelo de dados

- **Ambiente**: paredes retangulares (largura × profundidade × pé-direito × espessura) ou
  malha importada de um `.geoarq` (só referência visual).
- **Módulo paramétrico**: tipo, L × A × P, posição (x, y, z), rotação (0/90/180/270°),
  nº de portas, nº de gavetas, nº de prateleiras, altura do rodapé, materiais da caixa,
  da frente e do fundo.
- **Peça** (gerada pelo módulo): nome, comprimento × largura × espessura, material,
  fita de borda em cada um dos 4 lados, se respeita o veio, e a caixa 3D onde fica.
- **Ferragem**: dobradiças, corrediças, puxadores, pés reguláveis, suportes de prateleira.
- **Catálogo**: chapas (material, espessura, tamanho, preço), fitas (R$/m), ferragens (R$/un.),
  mão de obra e margem (%).

### Regras de construção (padrão de mercado, editáveis)

- Laterais inteiras; base e tampo entre as laterais; fundo de 6 mm encaixado em canal.
- Inferiores com rodapé recuado; aéreos sem rodapé.
- Portas e frentes de gaveta com folga de 2 mm por lado; sobreposição total.
- Dobradiças: 2 até 90 cm de altura de porta, 3 até 160 cm, 4 acima.
- Gaveta: frente + 2 laterais + traseira + fundo; corrediça telescópica do comprimento
  da profundidade útil arredondada para baixo (múltiplo de 5 cm).
- Fita de borda nas faces aparentes (frentes nos 4 lados; caixa só na frente).

## Plano de corte

- Agrupa peças por material + espessura.
- Chapa padrão 2750 × 1850 mm, refilo de 10 mm, espessura da serra 4 mm (editáveis).
- Algoritmo de guilhotina (*best area fit*, peças maiores primeiro); gira a peça quando o
  material não tem veio.
- Mostra cada chapa desenhada, com aproveitamento (%), e exporta **SVG** e **CSV**.

## Orçamento

- Chapas (inteiras ou por m² com perda), fita de borda (m), ferragens (un.),
  mão de obra (%) e margem (%). Exporta relatório **HTML** pronto para imprimir/PDF.

## Fases

1. **MVP** ✅ (esta versão)
   - Ambiente retangular, importação do `.geoarq` como referência
   - Módulos: balcão de portas, balcão gaveteiro, aéreo, torre/paneleiro, roupeiro,
     nicho/estante
   - Inserção com encaixe na parede e ao lado do último módulo; mover, girar, duplicar, excluir
   - Propriedades editáveis, lista de peças, plano de corte, orçamento, salvar/abrir `.planejei`
2. **Produção**: etiquetas de peças (com código), exportação para CNC/otimizadores
   (DXF por peça, CSV no formato do Corte Certo/Optimizer), furação de dobradiça e
   minifix, sobras de chapa reaproveitáveis.
3. **Projeto**: tampos (granito/MDF), cantos (L e U), paredes livres (não retangulares),
   portas de correr e de vidro, puxadores perfil, iluminação.
4. **Apresentação**: texturas de madeira (padrões Duratex/Guararapes/Arauco),
   render realista e vistas/elevações cotadas para a pasta do cliente.
5. **Loja**: catálogos de fabricantes, clientes, propostas e pedidos.
