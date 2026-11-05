//! A Windows notification icon is square, so a macOS-style wide title cannot
//! fit in it. Use the available 16 logical pixels for a legible two-line
//! amount, with the unabridged value remaining in the native tooltip.

use tauri::image::Image;

const SIZE: usize = 32;
const SCALE: usize = 2;
const LOGICAL: usize = SIZE / SCALE;

#[derive(Clone, Copy)]
struct Glyph {
    rows: [u8; 7],
    width: usize,
}

fn glyph(character: char, narrow: bool) -> Glyph {
    let (rows, width) = match character {
        '0' => ([6, 9, 9, 9, 9, 9, 6], 4),
        '1' => ([2, 6, 2, 2, 2, 2, 7], 4),
        '2' => ([6, 9, 1, 2, 4, 8, 15], 4),
        '3' => ([14, 1, 1, 6, 1, 1, 14], 4),
        '4' => ([2, 6, 10, 10, 15, 2, 2], 4),
        '5' => ([15, 8, 8, 14, 1, 1, 14], 4),
        '6' => ([6, 8, 8, 14, 9, 9, 6], 4),
        '7' => ([15, 1, 2, 2, 4, 4, 4], 4),
        '8' => ([6, 9, 9, 6, 9, 9, 6], 4),
        '9' => ([6, 9, 9, 7, 1, 1, 6], 4),
        '¥' => ([9, 6, 6, 15, 6, 15, 6], 4),
        '$' => ([2, 7, 10, 6, 5, 14, 4], 4),
        'k' => ([8, 8, 9, 10, 12, 10, 9], 4),
        'm' => ([0, 0, 13, 15, 15, 9, 9], 4),
        'b' => ([8, 8, 14, 9, 9, 9, 14], 4),
        '-' => ([0, 0, 0, 7, 0, 0, 0], 3),
        '.' => ([0, 0, 0, 0, 0, 1, 1], 1),
        _ => ([0; 7], 1),
    };
    if !narrow || width < 4 {
        return Glyph { rows, width };
    }
    Glyph {
        rows: rows.map(|row| ((row & 8) >> 1) | if row & 6 != 0 { 2 } else { 0 } | (row & 1)),
        width: 3,
    }
}

fn line_width(text: &str, narrow: bool) -> usize {
    let count = text.chars().count();
    text.chars().map(|c| glyph(c, narrow).width).sum::<usize>() + count.saturating_sub(1)
}

/// Returns the lines drawn in the icon. The complete exact balance stays in
/// the tooltip for large values, which are abbreviated to fit the tray slot.
fn lines(currency: &str, amount: f64) -> (String, Option<String>) {
    let symbol = match currency {
        "CNY" => "¥",
        "USD" => "$",
        _ => "",
    };
    let rounded = format!("{amount:.2}");
    let (integer, cents) = rounded.split_once('.').unwrap_or((&rounded, "00"));
    let full = format!("{symbol}{integer}");
    let top = if line_width(&full, true) <= LOGICAL {
        full
    } else {
        integer.to_string()
    };
    if line_width(&top, true) <= LOGICAL {
        return (top, Some(format!(".{cents}")));
    }
    let abs = amount.abs();
    let (divisor, suffix) = if abs >= 999_500_000.0 {
        (1_000_000_000.0, 'b')
    } else if abs >= 999_500.0 {
        (1_000_000.0, 'm')
    } else {
        (1_000.0, 'k')
    };
    let compact = format!("{:.0}{suffix}", amount / divisor);
    let with_symbol = format!("{symbol}{compact}");
    (
        if line_width(&with_symbol, true) <= LOGICAL {
            with_symbol
        } else {
            compact
        },
        None,
    )
}

fn draw_line(pixels: &mut [u8], text: &str, y: usize, color: [u8; 3]) {
    let narrow = line_width(text, false) > LOGICAL;
    let width = line_width(text, narrow);
    let mut x = LOGICAL.saturating_sub(width) / 2;
    for ch in text.chars() {
        let glyph = glyph(ch, narrow);
        for (row_index, row) in glyph.rows.iter().enumerate() {
            for column in 0..glyph.width {
                if row & (1 << (glyph.width - column - 1)) == 0 {
                    continue;
                }
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let px = (x + column) * SCALE + dx;
                        let py = (y + row_index) * SCALE + dy;
                        if px >= SIZE || py >= SIZE {
                            continue;
                        }
                        let at = (py * SIZE + px) * 4;
                        pixels[at..at + 3].copy_from_slice(&color);
                        pixels[at + 3] = 255;
                    }
                }
            }
        }
        x += glyph.width + 1;
    }
}

pub(crate) fn render(currency: &str, amount: f64, light: bool) -> Image<'static> {
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let color = if light { [28, 30, 33] } else { [255, 255, 255] };
    let (top, bottom) = lines(currency, amount);
    if let Some(bottom) = bottom {
        draw_line(&mut pixels, &top, 0, color);
        draw_line(&mut pixels, &bottom, 9, color);
    } else {
        draw_line(&mut pixels, &top, 4, color);
    }
    Image::new_owned(pixels, SIZE as u32, SIZE as u32)
}

#[cfg(target_os = "windows")]
pub(crate) fn taskbar_is_light() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
        .is_ok_and(|value| value != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_amounts_show_currency_and_both_decimal_digits() {
        assert_eq!(lines("CNY", 9.32), ("¥9".into(), Some(".32".into())));
        assert_eq!(lines("CNY", 99.84), ("¥99".into(), Some(".84".into())));
        assert_eq!(lines("USD", 2.5), ("$2".into(), Some(".50".into())));
        assert_eq!(lines("CNY", 1234.56), ("1234".into(), Some(".56".into())));
        assert_eq!(lines("CNY", 10_000.0), ("¥10k".into(), None));
    }

    #[test]
    fn fits_tray_slot_and_adapts_to_light_or_dark_taskbars() {
        for amount in [
            -1234.56,
            -9.32,
            0.0,
            9.32,
            99.84,
            123.45,
            1234.56,
            10_000.0,
            999_999_999.0,
        ] {
            let (top, bottom) = lines("CNY", amount);
            assert!(line_width(&top, true) <= LOGICAL, "{top}");
            assert!(bottom
                .as_ref()
                .is_none_or(|s| line_width(s, true) <= LOGICAL));
            let dark = render("CNY", amount, false);
            let light = render("CNY", amount, true);
            assert_eq!((dark.width(), dark.height()), (32, 32));
            assert!(dark
                .rgba()
                .chunks_exact(4)
                .any(|p| p == [255, 255, 255, 255]));
            assert!(light.rgba().chunks_exact(4).any(|p| p == [28, 30, 33, 255]));
            assert!(dark.rgba().chunks_exact(4).any(|p| p[3] == 0));
        }
        assert_ne!(
            render("CNY", 95.31, false).rgba(),
            render("CNY", 95.32, false).rgba()
        );
    }
}
