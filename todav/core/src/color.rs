//! An event's own colour: COLOR (RFC 7986), a CSS colour name. Nextcloud Calendar writes the
//! name closest to the colour picked; some clients write "#rrggbb" instead, which is read too.

/// The CSS colour names (CSS Color Module Level 4), with their colours. Of names for one colour
/// (aqua and cyan, gray and grey…) the first is written.
const NAMES: [(&str, u32); 148] = [
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

fn hex(rgb: u32) -> String {
    format!("#{rgb:06X}")
}

/// "#rgb" or "#rrggbb" (alpha, if any, dropped) as a number.
fn rgb(v: &str) -> Option<u32> {
    let h = v.strip_prefix('#')?;
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match h.len() {
        3 | 4 => {
            let d: Vec<u32> = h.chars().filter_map(|c| c.to_digit(16)).collect();
            Some(d[..3].iter().fold(0, |a, d| (a << 8) | (d * 17)))
        }
        6 | 8 => u32::from_str_radix(&h[..6], 16).ok(),
        _ => None,
    }
}

/// The colour of a COLOR value as "#RRGGBB": a CSS name, or a colour in hex as some clients
/// write it. None for anything else.
pub fn parse(v: &str) -> Option<String> {
    let v = v.trim();
    let name = v.to_ascii_lowercase();
    NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| *c)
        .or_else(|| rgb(v))
        .map(hex)
}

/// The CSS name closest to `color` ("#rrggbb"), to write as COLOR, as Nextcloud Calendar does.
pub fn name(color: &str) -> Option<&'static str> {
    let c = rgb(color)?;
    let part = |c: u32, shift: u32| ((c >> shift) & 0xFF) as i64;
    let distance = |n: u32| {
        [16, 8, 0]
            .iter()
            .map(|s| (part(c, *s) - part(n, *s)).pow(2))
            .sum::<i64>()
    };
    NAMES
        .iter()
        .min_by_key(|(_, n)| distance(*n))
        .map(|(name, _)| *name)
}

/// `color` as it will be shown once written: the colour of its closest CSS name.
pub fn snapped(color: &str) -> Option<String> {
    name(color).and_then(parse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_hex_are_read() {
        assert_eq!(parse("Chocolate").as_deref(), Some("#D2691E"));
        assert_eq!(parse("#d2691e").as_deref(), Some("#D2691E"));
        assert_eq!(parse("#f00").as_deref(), Some("#FF0000"));
        assert_eq!(parse("#0082C9FF").as_deref(), Some("#0082C9"));
        assert_eq!(parse("not a colour"), None);
        assert_eq!(parse("#12"), None);
    }

    #[test]
    fn the_closest_name_is_written() {
        // As Nextcloud Calendar's own tests have it.
        assert_eq!(name("#d2691e"), Some("chocolate"));
        assert_eq!(name("#d2699f"), Some("palevioletred"));
        // Of two names for one colour, the first.
        assert_eq!(name("#00FFFF"), Some("aqua"));
        assert_eq!(name("#808080"), Some("gray"));
        assert_eq!(
            snapped("#0082C9").as_deref(),
            parse(name("#0082C9").unwrap()).as_deref()
        );
    }
}
