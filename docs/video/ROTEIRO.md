# Planejei: vídeo de apresentação

- **Formato:** 16:9 (1920×1080), para YouTube, site e LinkedIn
- **Duração:** cerca de 75 a 85 segundos
- **Estilo:** narração em off sobre capturas reais do programa, com títulos animados e legendas

## 1. Texto para o ElevenLabs

Cole só o bloco abaixo. Cada parágrafo vira uma cena do vídeo. Os pontos finais e as quebras
de linha já criam as pausas.

```text
Projetar móveis planejados não precisa custar uma fortuna.

Conheça o Planejei. O programa que leva o seu projeto de marcenaria do desenho até o orçamento, em minutos.

Comece pelo ambiente. Informe as medidas da parede, ou importe o cômodo direto do GeoArq.

Depois, é só escolher na galeria: balcões, gaveteiros, aéreos, torres, roupeiros e nichos. Cada módulo encaixa sozinho na parede e se alinha ao vizinho.

Precisa ajustar? Mude largura, altura, profundidade, portas, gavetas e prateleiras. Troque o acabamento da caixa e das frentes. Tudo se recalcula na hora.

Quer ver por dentro? Esconda as portas com uma tecla e confira cada prateleira e divisória.

Enquanto você desenha, o Planejei monta a lista de peças, com medidas, fita de borda e ferragens.

E gera o plano de corte: cada chapa desenhada, com o desconto da serra, o refilo e o sentido do veio.

O orçamento sai pronto. Chapas, fita, ferragens, mão de obra e margem, tudo editável com os seus preços.

No fim, exporte a lista para a seccionadora, o plano de corte e uma proposta completa para o cliente.

Leve, rápido, e sem pagar caro por cada módulo.

Planejei. Do projeto ao corte, sem complicação.
```

### Configuração sugerida no ElevenLabs

- **Modelo:** *Eleven Multilingual v2*. Ele é o mais estável em português do Brasil.
- **Voz:** masculina ou feminina, tom de locução comercial, calma e próxima, não de anúncio
  de varejo. Teste duas ou três vozes com o primeiro parágrafo antes de gerar o resto.
- **Ajustes:**
  - *Stability*: entre 45 e 55%.
  - *Similarity*: em torno de 75%.
  - *Style*: entre 0 e 15%.
  - *Speaker boost*: ligado.
- **Velocidade:** normal (1.0). Se o áudio passar de 90 segundos, use 1.05.
- **Arquivo:** exporte em MP3 de 44,1 kHz ou em WAV. Se o ElevenLabs separar o áudio por
  parágrafo, me mande todos os arquivos, numerados.

Se alguma palavra sair com a pronúncia errada, troque-a pela forma escrita abaixo só no texto
do ElevenLabs:

| Palavra | Escrever como |
|---|---|
| GeoArq | Geo Arc |
| MDF | ême dê éfe |

## 2. Roteiro visual (cena por cena)

| # | Narração (resumo) | O que aparece na tela |
|---|---|---|
| 1 | Projetar móveis… fortuna | Fundo escuro. Frase em destaque, digitada palavra por palavra |
| 2 | Conheça o Planejei… | Logo e nome "Planejei". Abre a cozinha de exemplo em 3D com um giro lento da câmera |
| 3 | Comece pelo ambiente… | Painel *Ambiente* com as medidas e a sala vazia. Menção ao GeoArq com o ícone dele |
| 4 | Escolha na galeria… | Galeria à esquerda. Os módulos entram um a um e encaixam na parede |
| 5 | Precisa ajustar? | Painel de propriedades. Largura e número de gavetas mudando, acabamento trocado para Grafite e Carvalho |
| 6 | Quer ver por dentro? | Tecla **F** na tela. Portas somem e aparecem prateleiras e divisórias |
| 7 | Lista de peças | Janela *Lista de peças* rolando, com as colunas medida, fita e material destacadas |
| 8 | Plano de corte | Aba *Plano de corte*. Zoom numa chapa com o selo "% de aproveitamento" |
| 9 | Orçamento | Aba *Orçamento*. O total **R$ 6.111,55** sobe contando |
| 10 | Exporte… | Três cartões entram: CSV, SVG e proposta em HTML aberta no navegador |
| 11 | Leve, rápido… | Comparação simples: "Tudo incluso · Nativo e leve · Seus preços" |
| 12 | Planejei. Do projeto ao corte… | Logo, slogan e chamada final (site ou contato, a definir) |

**Ao longo do vídeo:**
- Legenda em todas as falas.
- Trilha leve e instrumental, com volume baixo sob a voz.
- Cores do vídeo seguem as da interface: claro, azul de seleção e madeira.

## 3. Próximos passos

1. Gerar o áudio no ElevenLabs com o texto da seção 1 e me enviar o arquivo.
2. Com o áudio em mãos:
   - Meço o tempo de cada parágrafo.
   - Gravo as capturas de cada cena com o próprio Planejei, pelo modo de captura automática.
   - Monto o vídeo com o Hyperframes (HTML), com títulos, legendas e transições sincronizados
     com a narração.
   - Exporto em MP4.
3. Ainda falta você me passar:
   - A chamada final (site, Instagram ou WhatsApp).
   - Se quer uma trilha específica. Sem isso, deixo o espaço para você colocar uma trilha
     livre de direitos.

## 4. Como o vídeo é gerado

- `audio/narracao.mp3` é a narração gerada no ElevenLabs.
- `composicao/index.html` é a animação. Abra no navegador para ver em tempo real, ou use
  `?t=12.5` para congelar num instante. Os tempos de cada parágrafo ficam na constante `B`.
- `composicao/render.mjs` desenha quadro a quadro no Chrome e junta com o áudio no ffmpeg.
- As capturas em `composicao/img/` saem do próprio Planejei:
  `PLANEJEI_SIZE=1600x900 PLANEJEI_DEMO=kitchen PLANEJEI_SCREENSHOT=img/s02.png`, com
  `PLANEJEI_TAB`, `PLANEJEI_SELECT`, `PLANEJEI_HIDE_FRONTS` e `PLANEJEI_PARTS` para cada cena.
  `PLANEJEI_DUMP=pasta` grava o projeto e as exportações.

```bash
cd docs/video/composicao && yarn install
```

```bash
cd docs/video/composicao && yarn render
```

`yarn render --from 30 --to 40` gera só um trecho, e `yarn render --stills 5,20` gera só os PNGs
desses instantes.
