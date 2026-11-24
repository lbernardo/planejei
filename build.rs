// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! No Windows, embute o ícone e as informações do programa no Planejei.exe
//! (inclusive compilando a partir do Mac, com o windres do MinGW).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico")
        .set("ProductName", "Planejei")
        .set("FileDescription", "Planejei — móveis planejados com plano de corte e orçamento")
        .set("OriginalFilename", "Planejei.exe");
    if let Err(e) = res.compile() {
        // Sem o compilador de recursos o .exe sai sem ícone, mas funciona.
        println!("cargo:warning=ícone do Windows não embutido: {e}");
    }
}
