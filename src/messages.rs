// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use pumpkin_plugin_api::{common::RgbColor, text::TextComponent};

const BRAND: &str = "WORLDPUMPKIN";
const BRAND_START: RgbColor = rgb(255, 104, 24);
const BRAND_END: RgbColor = rgb(255, 190, 64);

#[derive(Clone, Copy)]
pub enum MessageKind {
    Info,
    Error,
}

pub fn prefixed(kind: MessageKind, message: &str) -> TextComponent {
    let root = TextComponent::text("");
    // Pumpkin's Wasm API has RGB components, but no MiniMessage parser.
    append_gradient(&root, BRAND);
    append_colored(&root, " › ", rgb(255, 156, 42), true);
    append_colored(&root, message, body_color(kind), false);
    root
}

fn append_gradient(root: &TextComponent, text: &str) {
    let chars = text.chars().count().saturating_sub(1).max(1);
    for (index, character) in text.chars().enumerate() {
        let color = gradient_color(index, chars);
        append_colored(root, &character.to_string(), color, true);
    }
}

fn append_colored(root: &TextComponent, text: &str, color: RgbColor, bold: bool) {
    let child = TextComponent::text(text);
    child.color_rgb(color);
    child.bold(bold);
    root.add_child(child);
}

fn gradient_color(index: usize, max_index: usize) -> RgbColor {
    lerp(BRAND_START, BRAND_END, index, max_index)
}

fn lerp(start: RgbColor, end: RgbColor, pos: usize, max: usize) -> RgbColor {
    rgb(
        lerp_channel(start.r, end.r, pos, max),
        lerp_channel(start.g, end.g, pos, max),
        lerp_channel(start.b, end.b, pos, max),
    )
}

fn lerp_channel(start: u8, end: u8, pos: usize, max: usize) -> u8 {
    let start = start as i32;
    let end = end as i32;
    (start + ((end - start) * pos as i32 / max as i32)) as u8
}

fn body_color(kind: MessageKind) -> RgbColor {
    match kind {
        MessageKind::Info => rgb(238, 238, 238),
        MessageKind::Error => rgb(255, 105, 97),
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> RgbColor {
    RgbColor { r, g, b }
}
