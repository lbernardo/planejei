#!/usr/bin/env bash
# Gera os instaláveis do Planejei para macOS e Windows a partir deste Mac.
#
#   ./scripts/build.sh            # mac + windows
#   ./scripts/build.sh mac        # só macOS (Apple Silicon + Intel, "universal")
#   ./scripts/build.sh windows    # só Windows (64 bits)
#
# Saída em dist/:
#   Planejei-<versão>-mac.zip      → Planejei.app
#   Planejei-<versão>-mac.dmg      → Planejei.app + atalho para Aplicativos
#   Planejei-<versão>-windows.zip  → Planejei.exe (portátil, sem instalar)
#   Planejei-<versão>-setup.exe    → instalador do Windows (precisa do NSIS: brew install makensis)
set -euo pipefail

cd "$(dirname "$0")/.."
APP="Planejei"
BIN="planejei"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DIST="dist"
WHAT="${1:-all}"

say()  { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
fail() { printf '\n\033[1;31mErro:\033[0m %s\n' "$*" >&2; exit 1; }

need_target() {
  if ! rustup target list --installed | grep -qx "$1"; then
    say "Instalando o alvo Rust $1"
    rustup target add "$1"
  fi
}

build_mac() {
  need_target aarch64-apple-darwin
  need_target x86_64-apple-darwin

  say "Compilando para Mac (Apple Silicon)"
  cargo build --release --target aarch64-apple-darwin
  say "Compilando para Mac (Intel)"
  cargo build --release --target x86_64-apple-darwin

  local app="$DIST/mac/$APP.app"
  rm -rf "$DIST/mac" && mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

  say "Juntando os dois em um binário universal"
  lipo -create \
    "target/aarch64-apple-darwin/release/$BIN" \
    "target/x86_64-apple-darwin/release/$BIN" \
    -output "$app/Contents/MacOS/$APP"

  cp assets/icon.icns "$app/Contents/Resources/$APP.icns"
  cp LICENSE THIRD_PARTY.md THIRD_PARTY-pt-br.md "$app/Contents/Resources/"

  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP</string>
  <key>CFBundleDisplayName</key><string>$APP</string>
  <key>CFBundleIdentifier</key><string>local.planejei.app</string>
  <key>CFBundleExecutable</key><string>$APP</string>
  <key>CFBundleIconFile</key><string>$APP</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleDevelopmentRegion</key><string>pt-BR</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHumanReadableCopyright</key><string>Copyright (C) 2026 Lucas Bernardo. GPL-3.0-or-later.</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

  # Assinatura local ("ad hoc"): obrigatória para rodar em Apple Silicon depois do lipo.
  codesign --force --deep --sign - "$app" >/dev/null

  local zip="$DIST/$APP-$VERSION-mac.zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$app" "$zip"
  say "Mac pronto: $zip"

  say "Gerando o .dmg (arrastar para Aplicativos)"
  local stage="$DIST/mac/dmg"
  rm -rf "$stage" && mkdir -p "$stage"
  ditto "$app" "$stage/$APP.app"
  ln -s /Applications "$stage/Aplicativos"
  cp LICENSE "$stage/LICENSE.txt"
  local dmg="$DIST/$APP-$VERSION-mac.dmg"
  rm -f "$dmg"
  hdiutil create -quiet -volname "$APP $VERSION" -srcfolder "$stage" -fs HFS+ -format UDZO "$dmg"
  rm -rf "$stage"
  say "DMG pronto: $dmg"
  lipo -info "$app/Contents/MacOS/$APP"
}

build_windows() {
  local target=x86_64-pc-windows-gnu
  need_target "$target"
  if ! command -v x86_64-w64-mingw32-gcc >/dev/null; then
    fail "falta o MinGW-w64 (o 'linker' do Windows). Instale uma vez com:
    brew install mingw-w64"
  fi

  say "Compilando para Windows (64 bits)"
  cargo build --release --target "$target"

  rm -rf "$DIST/windows" && mkdir -p "$DIST/windows"
  cp "target/$target/release/$BIN.exe" "$DIST/windows/$APP.exe"
  # Texto com quebras de linha do Windows, para abrir certo no Bloco de Notas.
  awk '{ printf "%s\r\n", $0 }' LICENSE > "$DIST/windows/LICENSE.txt"
  cp THIRD_PARTY.md THIRD_PARTY-pt-br.md "$DIST/windows/"

  local zip="$DIST/$APP-$VERSION-windows.zip"
  rm -f "$zip"
  (cd "$DIST/windows" && zip -q -9 "../$(basename "$zip")" ./*)
  say "Windows pronto: $zip"

  if command -v makensis >/dev/null; then
    say "Gerando o instalador do Windows"
    local setup="$PWD/$DIST/$APP-$VERSION-setup.exe"
    # O NSIS precisa de locale UTF-8 para ler o script (acentos).
    LC_ALL=en_US.UTF-8 makensis -V2 -DVERSION="$VERSION" -DSRC="$PWD/$DIST/windows" -DOUT="$setup" installer/planejei.nsi
    say "Instalador pronto: $DIST/$(basename "$setup")"
  else
    say "NSIS não encontrado: instalador não gerado (instale com: brew install makensis)"
  fi
}

say "Planejei $VERSION — atualizando as licenças de terceiros"
./scripts/third_party.sh
say "Rodando os testes"
cargo test --release --quiet

mkdir -p "$DIST"
case "$WHAT" in
  mac)     build_mac ;;
  windows) build_windows ;;
  all)     build_mac; build_windows ;;
  *)       fail "uso: $0 [mac|windows|all]" ;;
esac
ls -lh "$DIST"/*.zip "$DIST"/*.dmg "$DIST"/*.exe 2>/dev/null || true
