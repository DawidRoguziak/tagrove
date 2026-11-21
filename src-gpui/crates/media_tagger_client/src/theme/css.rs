//! A small reader for the subset of CSS `src/styles.css` uses for its tokens, for the
//! parity tests: rules with their declarations, and values built from hex colours,
//! `transparent`, `rgb()`, `var()`, `color-mix()`, pixel lengths, milliseconds,
//! `cubic-bezier()` and box-shadow lists.

use std::{collections::HashMap, time::Duration};

use super::{CubicBezier, Rgba, Shadow};

/// One rule: its prelude (selector or at-rule) and its own declarations, without those
/// of nested rules.
#[derive(Debug, Clone)]
pub struct Rule {
    pub prelude: String,
    pub declarations: Vec<(String, String)>,
}

impl Rule {
    pub fn get(&self, property: &str) -> Option<&str> {
        self.declarations
            .iter()
            .rev()
            .find(|(name, _)| name == property)
            .map(|(_, value)| value.as_str())
    }
}

/// Every rule of a stylesheet, nested ones included, in source order.
pub fn rules(css: &str) -> Vec<Rule> {
    let css = strip_comments(css);
    let bytes = css.as_bytes();
    let mut rules = Vec::new();
    // Open blocks: (index of the rule in `rules`, start of its body).
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut boundary = 0;
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match (quote, byte) {
            (Some(q), _) if byte == q => quote = None,
            (Some(_), b'\\') => index += 1,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'{') => {
                rules.push(Rule {
                    prelude: css[boundary..index].trim().to_owned(),
                    declarations: Vec::new(),
                });
                open.push((rules.len() - 1, index + 1));
                boundary = index + 1;
            }
            (None, b'}') => {
                if let Some((rule, start)) = open.pop() {
                    rules[rule].declarations = declarations(&css[start..index]);
                }
                boundary = index + 1;
            }
            (None, b';') => boundary = index + 1,
            _ => {}
        }
        index += 1;
    }
    rules
}

fn strip_comments(css: &str) -> String {
    let mut output = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        output.push_str(&rest[..start]);
        rest = rest[start + 2..]
            .find("*/")
            .map_or("", |end| &rest[start + 2 + end + 2..]);
    }
    output.push_str(rest);
    output
}

/// The declarations at depth 0 of a block body.
fn declarations(body: &str) -> Vec<(String, String)> {
    let mut depth = 0usize;
    let mut flat = String::new();
    let mut quote = None;
    for character in body.chars() {
        match (quote, character) {
            (Some(q), _) if character == q => {
                quote = None;
            }
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{') => {
                depth += 1;
                // A nested rule: drop its prelude, which has no terminating `;`.
                if depth == 1 {
                    if let Some(cut) = flat.rfind(';') {
                        flat.truncate(cut + 1);
                    } else {
                        flat.clear();
                    }
                }
                continue;
            }
            (None, '}') => {
                depth = depth.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        if depth == 0 {
            flat.push(character);
        }
    }
    split_top_level(&flat, ';')
        .into_iter()
        .filter_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            let name = name.trim();
            (!name.is_empty()).then(|| (name.to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// `value` split at `separator` outside parentheses and quotes.
pub fn split_top_level(value: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote = None;
    let mut start = 0;
    for (index, character) in value.char_indices() {
        match (quote, character) {
            (Some(q), _) if character == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '(') => depth += 1,
            (None, ')') => depth = depth.saturating_sub(1),
            (None, c) if c == separator && depth == 0 => {
                parts.push(&value[start..index]);
                start = index + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

/// Whitespace-separated words outside parentheses.
fn words(value: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (index, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c.is_whitespace() && depth == 0 => {
                if let Some(from) = start.take() {
                    words.push(&value[from..index]);
                }
                continue;
            }
            _ => {}
        }
        start.get_or_insert(index);
    }
    if let Some(from) = start {
        words.push(&value[from..]);
    }
    words
}

/// The arguments of `name(...)` when `value` is exactly such a call.
fn call<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    value
        .strip_prefix(name)?
        .strip_prefix('(')?
        .strip_suffix(')')
}

/// Custom properties visible to a theme, later sources overriding earlier ones.
pub type Scope = HashMap<String, String>;

pub fn resolve_var<'a>(scope: &'a Scope, value: &'a str) -> Result<&'a str, String> {
    let mut value = value.trim();
    for _ in 0..16 {
        let Some(arguments) = call(value, "var") else {
            return Ok(value);
        };
        let mut parts = split_top_level(arguments, ',').into_iter();
        let name = parts.next().unwrap_or_default().trim();
        let fallback = parts.next().map(str::trim);
        value = match (scope.get(name), fallback) {
            (Some(next), _) => next.trim(),
            (None, Some(fallback)) => fallback,
            (None, None) => return Err(format!("{name} is not defined")),
        };
    }
    Err(format!("var() nests too deep at {value}"))
}

/// A colour with alpha in floating point, as CSS computes it before drawing.
#[derive(Debug, Clone, Copy)]
struct Color {
    rgb: [f64; 3],
    alpha: f64,
}

impl Color {
    fn to_rgba(self) -> Rgba {
        let byte = |value: f64| (value.clamp(0.0, 255.0)).round() as u8;
        if self.alpha <= 0.0 {
            return Rgba::TRANSPARENT;
        }
        Rgba {
            r: byte(self.rgb[0]),
            g: byte(self.rgb[1]),
            b: byte(self.rgb[2]),
            a: byte(self.alpha * 255.0),
        }
    }
}

pub fn color(scope: &Scope, value: &str) -> Result<Rgba, String> {
    color_value(scope, value).map(Color::to_rgba)
}

fn color_value(scope: &Scope, value: &str) -> Result<Color, String> {
    let value = resolve_var(scope, value)?;
    if value == "transparent" {
        return Ok(Color {
            rgb: [0.0; 3],
            alpha: 0.0,
        });
    }
    if let Some(digits) = value.strip_prefix('#') {
        return hex(digits).ok_or_else(|| format!("bad hex colour {value}"));
    }
    if let Some(arguments) = call(value, "rgb") {
        let (channels, alpha) = match arguments.split_once('/') {
            Some((channels, alpha)) => (channels, alpha_value(alpha.trim())?),
            None => (arguments, 1.0),
        };
        let channels: Vec<f64> = channels
            .split_whitespace()
            .map(|channel| {
                channel
                    .parse::<f64>()
                    .map_err(|_| format!("bad rgb() {value}"))
            })
            .collect::<Result<_, _>>()?;
        let [r, g, b] = channels[..] else {
            return Err(format!("rgb() needs three channels: {value}"));
        };
        return Ok(Color {
            rgb: [r, g, b],
            alpha,
        });
    }
    if let Some(arguments) = call(value, "color-mix") {
        return color_mix(scope, arguments);
    }
    Err(format!("unsupported colour {value}"))
}

fn hex(digits: &str) -> Option<Color> {
    if !digits.is_ascii() {
        return None;
    }
    let channel = |text: &str| u8::from_str_radix(text, 16).ok().map(f64::from);
    let rgb = match digits.len() {
        3 => [
            channel(&digits[0..1])? * 17.0,
            channel(&digits[1..2])? * 17.0,
            channel(&digits[2..3])? * 17.0,
        ],
        6 => [
            channel(&digits[0..2])?,
            channel(&digits[2..4])?,
            channel(&digits[4..6])?,
        ],
        _ => return None,
    };
    Some(Color { rgb, alpha: 1.0 })
}

fn alpha_value(text: &str) -> Result<f64, String> {
    match text.strip_suffix('%') {
        Some(percent) => percent.parse::<f64>().map(|p| p / 100.0),
        None => text.parse::<f64>(),
    }
    .map_err(|_| format!("bad alpha {text}"))
}

/// `color-mix(in <space>, <colour> [<p>%], <colour> [<p>%])` with premultiplied
/// alpha. In `oklab` only a mix with a fully transparent colour is supported, which
/// keeps the colour and scales its alpha in every space.
fn color_mix(scope: &Scope, arguments: &str) -> Result<Color, String> {
    let parts = split_top_level(arguments, ',');
    let [space, first, second] = parts[..] else {
        return Err(format!("color-mix() needs three arguments: {arguments}"));
    };
    let space = space
        .trim()
        .strip_prefix("in ")
        .ok_or_else(|| format!("color-mix() without a space: {arguments}"))?
        .trim();
    let component = |text: &str| -> Result<(Color, Option<f64>), String> {
        let parts = words(text.trim());
        match parts[..] {
            [colour] => Ok((color_value(scope, colour)?, None)),
            [colour, percent] => Ok((color_value(scope, colour)?, Some(alpha_value(percent)?))),
            _ => Err(format!("bad color-mix() component {text}")),
        }
    };
    let (a, pa) = component(first)?;
    let (b, pb) = component(second)?;
    let (pa, pb) = match (pa, pb) {
        (Some(pa), Some(pb)) => (pa, pb),
        (Some(pa), None) => (pa, 1.0 - pa),
        (None, Some(pb)) => (1.0 - pb, pb),
        (None, None) => (0.5, 0.5),
    };
    let one_side_transparent = a.alpha == 0.0 || b.alpha == 0.0;
    if space != "srgb" && !one_side_transparent {
        return Err(format!(
            "color-mix() in {space} of two colours is not supported"
        ));
    }
    let alpha = a.alpha * pa + b.alpha * pb;
    if alpha == 0.0 {
        return Ok(Color {
            rgb: [0.0; 3],
            alpha: 0.0,
        });
    }
    let rgb = [0, 1, 2].map(|i| (a.rgb[i] * a.alpha * pa + b.rgb[i] * b.alpha * pb) / alpha);
    Ok(Color { rgb, alpha })
}

/// `0` or `<n>px`.
pub fn px(scope: &Scope, value: &str) -> Result<f32, String> {
    let value = resolve_var(scope, value)?;
    if value == "0" {
        return Ok(0.0);
    }
    value
        .strip_suffix("px")
        .and_then(|number| number.parse().ok())
        .ok_or_else(|| format!("not a pixel length: {value}"))
}

/// `<n>ms` or `<n>s`.
pub fn duration(scope: &Scope, value: &str) -> Result<Duration, String> {
    let value = resolve_var(scope, value)?;
    let millis = if let Some(ms) = value.strip_suffix("ms") {
        ms.parse::<f64>().ok()
    } else {
        value
            .strip_suffix('s')
            .and_then(|s| s.parse::<f64>().ok())
            .map(|seconds| seconds * 1000.0)
    };
    millis
        .map(|millis| Duration::from_secs_f64(millis / 1000.0))
        .ok_or_else(|| format!("not a duration: {value}"))
}

pub fn cubic_bezier(scope: &Scope, value: &str) -> Result<CubicBezier, String> {
    let value = resolve_var(scope, value)?;
    let arguments =
        call(value, "cubic-bezier").ok_or_else(|| format!("not a cubic-bezier: {value}"))?;
    let numbers: Vec<f32> = split_top_level(arguments, ',')
        .into_iter()
        .map(|number| {
            number
                .trim()
                .parse()
                .map_err(|_| format!("bad number in {value}"))
        })
        .collect::<Result<_, _>>()?;
    let [x1, y1, x2, y2] = numbers[..] else {
        return Err(format!("cubic-bezier needs four numbers: {value}"));
    };
    Ok(CubicBezier { x1, y1, x2, y2 })
}

/// A `box-shadow` list; `none` is empty.
pub fn shadows(scope: &Scope, value: &str) -> Result<Vec<Shadow>, String> {
    let value = resolve_var(scope, value)?;
    if value == "none" {
        return Ok(Vec::new());
    }
    split_top_level(value, ',')
        .into_iter()
        .map(|layer| {
            let mut inset = false;
            let mut lengths = Vec::new();
            let mut colour = None;
            for word in words(layer.trim()) {
                if word == "inset" {
                    inset = true;
                } else if let Ok(length) = px(scope, word) {
                    lengths.push(length);
                } else if colour.replace(color(scope, word)?).is_some() {
                    return Err(format!("two colours in shadow {layer}"));
                }
            }
            let (offset_x, offset_y, blur, spread) = match lengths[..] {
                [x, y] => (x, y, 0.0, 0.0),
                [x, y, blur] => (x, y, blur, 0.0),
                [x, y, blur, spread] => (x, y, blur, spread),
                _ => return Err(format!("bad shadow lengths in {layer}")),
            };
            Ok(Shadow {
                inset,
                offset_x,
                offset_y,
                blur,
                spread,
                color: colour.ok_or_else(|| format!("shadow without a colour: {layer}"))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{color, rules, shadows, Scope};
    use crate::theme::{Rgba, Shadow};

    #[test]
    fn rules_keep_their_own_declarations_only() {
        let css = r#"
@import "x";
/* a { b: c } */
:root { --a: #fff; --url: url("data:a;b{c}"); }
@layer base {
  body { margin: 0; }
  h1, h2 { font-size: 24px; }
}
"#;
        let rules = rules(css);
        let names: Vec<&str> = rules.iter().map(|rule| rule.prelude.as_str()).collect();

        assert_eq!(names, [":root", "@layer base", "body", "h1, h2"]);
        assert_eq!(rules[0].get("--a"), Some("#fff"));
        assert_eq!(rules[0].get("--url"), Some(r#"url("data:a;b{c}")"#));
        assert!(rules[1].declarations.is_empty());
        assert_eq!(rules[3].get("font-size"), Some("24px"));
    }

    #[test]
    fn colours_resolve_variables_and_mixes() {
        let scope: Scope = [
            ("--p", "#10b981"),
            ("--s", "var(--base)"),
            ("--base", "#202224"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

        assert_eq!(color(&scope, "var(--s)"), Ok(Rgba::hex(0x202224)));
        assert_eq!(color(&scope, "var(--none, #0af)"), Ok(Rgba::hex(0x00aaff)));
        assert!(color(&scope, "var(--none)").is_err());
        assert_eq!(
            color(&scope, "color-mix(in srgb, var(--p) 14%, var(--s))"),
            Ok(Rgba::hex(0x10b981).mix_srgb(14, Rgba::hex(0x202224)))
        );
        assert_eq!(
            color(&scope, "color-mix(in oklab, var(--p) 16%, transparent)"),
            Ok(Rgba::hex(0x10b981).with_alpha_percent(16))
        );
        assert!(color(&scope, "color-mix(in oklab, var(--p) 16%, var(--s))").is_err());
        assert_eq!(
            color(&scope, "rgb(0 0 0 / 12%)"),
            Ok(Rgba::hex(0).with_alpha_percent(12))
        );
        assert_eq!(color(&scope, "transparent"), Ok(Rgba::TRANSPARENT));
    }

    #[test]
    fn shadow_lists_parse_layer_by_layer() {
        let scope: Scope = [("--c", "#000")]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();

        assert_eq!(shadows(&scope, "none"), Ok(vec![]));
        assert_eq!(
            shadows(
                &scope,
                "inset 0 0 0 1px var(--c), 0 4px 16px rgb(0 0 0 / 12%)"
            ),
            Ok(vec![
                Shadow::ring(true, 1.0, Rgba::hex(0)),
                Shadow::drop(4.0, 16.0, Rgba::hex(0).with_alpha_percent(12)),
            ])
        );
        assert!(shadows(&scope, "0 1px").is_err(), "no colour");
    }
}
