# Planejei: presentation video

**English** · [Português (Brasil)](SCRIPT-pt-br.md)

Finished video: [planejei-apresentacao.mp4](planejei-apresentacao.mp4) (77 s, 1920×1080).

- **Format:** 16:9 (1920×1080), for YouTube, a website and LinkedIn.
- **Length:** 77 seconds (74 s of narration).
- **Style:** voice-over on top of real captures of the app, with animated titles and captions.
- **Language:** the narration and on-screen text are in Brazilian Portuguese, the language of
  the app and its audience.

## 1. Narration (ElevenLabs)

Paste only the block below into ElevenLabs. Each paragraph becomes one scene. Full stops and
blank lines create the pauses.

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

### English translation (reference only)

1. Designing custom furniture doesn't have to cost a fortune.
2. Meet Planejei. The program that takes your cabinetry project from drawing to quote, in
   minutes.
3. Start with the room. Enter the wall measurements, or import the room straight from GeoArq.
4. Then just pick from the gallery: base units, drawer units, wall units, tall units, wardrobes
   and shelves. Each module snaps to the wall on its own and lines up with its neighbour.
5. Need to adjust? Change width, height, depth, doors, drawers and shelves. Swap the carcass and
   front finishes. Everything recalculates instantly.
6. Want to see inside? Hide the doors with one key and check every shelf and divider.
7. While you design, Planejei builds the parts list, with measurements, edge banding and
   hardware.
8. And it generates the cutting plan: every sheet drawn, accounting for the saw kerf, the trim
   and the grain direction.
9. The quote comes out ready. Sheets, edge banding, hardware, labour and margin, all editable
   with your own prices.
10. Finally, export the list for the panel saw, the cutting plan and a complete proposal for the
    client.
11. Lightweight, fast, and no paying a fortune for each module.
12. Planejei. From design to cutting, without the hassle.

### Suggested ElevenLabs settings

- **Model:** *Eleven Multilingual v2*, the most stable one for Brazilian Portuguese.
- **Voice:**
  - Male or female, with a commercial voice-over tone.
  - Calm and friendly, not a retail-ad voice.
  - Try two or three voices with the first paragraph before generating the rest.
- **Settings:**
  - *Stability*: 45–55%.
  - *Similarity*: about 75%.
  - *Style*: 0–15%.
  - *Speaker boost*: on.
- **Speed:** normal (1.0). If the audio runs over 90 seconds, use 1.05.
- **File:** export as 44.1 kHz MP3 or WAV.

If a word is mispronounced, replace it with the spelling below, only in the text you paste into
ElevenLabs:

| Word | Write it as |
|---|---|
| GeoArq | Geo Arc |
| MDF | ême dê éfe |

## 2. Storyboard (scene by scene)

| # | Narration (summary) | On screen |
|---|---|---|
| 1 | Designing… a fortune | Dark background. The sentence appears word by word, with "fortuna" in a wood colour |
| 2 | Meet Planejei… | Icon, name and "Do desenho ao orçamento, em minutos" (from drawing to quote, in minutes). Then the sample kitchen in 3D with a slow zoom |
| 3 | Start with the room… | Empty room. The *Ambiente* (room) panel is highlighted ("Medidas do ambiente", room measurements), then the import button ("ou importe do GeoArq", or import from GeoArq) |
| 4 | Pick from the gallery… | The gallery is highlighted. Modules appear one by one, with labels for each type. Ends with "Encaixa sozinho na parede" (snaps to the wall on its own) |
| 5 | Need to adjust? | Zoom into the drawer unit's properties panel. Then the fronts change from Graphite to Oak and the unit gets 4 drawers |
| 6 | Want to see inside? | The **F** key appears and is "pressed". Doors and fronts disappear, revealing shelves and dividers |
| 7 | Parts list | Zoom into the *Lista de peças* (parts list) window, with labels for measurements, edge banding and hardware |
| 8 | Cutting plan | *Plano de corte* (cutting plan) tab, zooming into the first sheet, with labels for the 4 mm kerf, 10 mm trim and grain direction |
| 9 | Quote | *Orçamento* (quote) tab, zooming into the table. A card counts the total up to **R$ 6.111,55** |
| 10 | Export… | Three cards: parts list (CSV), cutting plan (SVG) and proposal (HTML) |
| 11 | Lightweight, fast… | "Leve. Rápido. Sem pagar caro por módulo." (Lightweight. Fast. No paying a fortune per module.) |
| 12 | Planejei. From design to cutting… | Icon, name and tagline |

**Throughout:**
- Every line is captioned.
- Colours follow the app's interface: light background, selection blue and wood.
- No music yet (see section 3).

## 3. Status

- [x] Narration generated in ElevenLabs (voice "Marcio", `audio/narracao.mp3`).
- [x] Timing of each paragraph measured from the pauses in the audio.
- [x] Every scene captured from Planejei itself.
- [x] Video assembled and exported as MP4, with the new icon.
- [ ] Final call to action (website, Instagram or WhatsApp).
- [ ] Royalty-free instrumental soundtrack, mixed low under the voice.

## 4. How the video is built

- `audio/narracao.mp3` is the ElevenLabs narration.
- `composicao/index.html` is the animation.
  - Open it in a browser to watch it in real time, or add `?t=12.5` to freeze a single moment.
  - The start time of each paragraph lives in the `B` constant.
- `composicao/render.mjs` draws the animation frame by frame in Chrome and muxes it with the
  audio in ffmpeg.
- The captures in `composicao/img/` come from Planejei itself.
  - Base command: `PLANEJEI_SIZE=1600x900 PLANEJEI_DEMO=kitchen PLANEJEI_SCREENSHOT=img/s02.png`.
  - Each scene adds `PLANEJEI_TAB`, `PLANEJEI_SELECT`, `PLANEJEI_HIDE_FRONTS` or `PLANEJEI_PARTS`.
  - `PLANEJEI_DUMP=folder` writes the project and its exports.

```bash
cd docs/video/composicao && yarn install
```

```bash
cd docs/video/composicao && yarn render
```

`yarn render --from 30 --to 40` renders only that segment, and `yarn render --stills 5,20` writes
only PNGs of those moments.
