# Planejei: vídeo de apresentação

[English](SCRIPT.md) · **Português (Brasil)**

Vídeo pronto: [planejei-apresentacao.mp4](planejei-apresentacao.mp4) (77 s, 1920×1080).

- **Formato:** 16:9 (1920×1080), para YouTube, site e LinkedIn
- **Duração:** 77 segundos (narração de 74 s)
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
| 1 | Projetar móveis… fortuna | Fundo escuro. A frase entra palavra por palavra, com "fortuna" em cor de madeira |
| 2 | Conheça o Planejei… | Ícone, nome e "Do desenho ao orçamento, em minutos". Depois, a cozinha de exemplo em 3D com zoom lento |
| 3 | Comece pelo ambiente… | Sala vazia. Destaque no painel *Ambiente* ("Medidas do ambiente") e depois no botão de importar ("ou importe do GeoArq") |
| 4 | Escolha na galeria… | Galeria destacada. Os módulos entram um a um, com etiquetas: balcões, gaveteiros, aéreos, torres, roupeiros, nichos. Fecha com "Encaixa sozinho na parede" |
| 5 | Precisa ajustar? | Zoom no painel de propriedades do gaveteiro. Depois, as frentes trocam de Grafite para Carvalho e o gaveteiro passa a 4 gavetas |
| 6 | Quer ver por dentro? | A tecla **F** aparece e é "pressionada". Portas e frentes somem e aparecem prateleiras e divisórias |
| 7 | Lista de peças | Zoom na janela *Lista de peças*, com as etiquetas medidas, fita de borda e ferragens |
| 8 | Plano de corte | Aba *Plano de corte* com zoom na primeira chapa e as etiquetas serra 4 mm, refilo 10 mm e sentido do veio |
| 9 | Orçamento | Aba *Orçamento* com zoom na tabela. Um cartão mostra o total subindo até **R$ 6.111,55** |
| 10 | Exporte… | Três cartões: lista de peças (CSV), plano de corte (SVG) e proposta (HTML) |
| 11 | Leve, rápido… | "Leve. Rápido. Sem pagar caro por módulo." |
| 12 | Planejei. Do projeto ao corte… | Ícone, nome e slogan |

**Ao longo do vídeo:**
- Legenda em todas as falas.
- Sem trilha por enquanto (ver a seção 3).
- Cores do vídeo seguem as da interface: claro, azul de seleção e madeira.

## 3. Situação

- [x] Narração gerada no ElevenLabs (voz "Marcio", `audio/narracao.mp3`).
- [x] Tempos de cada parágrafo medidos pelas pausas do áudio.
- [x] Capturas de cada cena gravadas com o próprio Planejei.
- [x] Vídeo montado e exportado em MP4, já com o ícone novo.
- [ ] Chamada final (site, Instagram ou WhatsApp).
- [ ] Trilha sonora instrumental, livre de direitos, com volume baixo sob a voz.

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
