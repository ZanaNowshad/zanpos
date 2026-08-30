//! Pack size, which is half of a product's identity.
//!
//! "Almarai Strawberry Milk" is not a product. `6x200ml` and `1L` are different
//! things at different prices, and a comparison that ignores the difference
//! reports a competitor as 40% cheaper when they are simply selling a smaller
//! carton. Every source states size differently — Akelny gives a dedicated
//! `6x200ml` line, Bahrain Pharmacy buries it in the name as `4.8 G`, and some
//! names carry two (`0.10OZ 3 G`) — so this normalises all of them to one
//! comparable number.
//!
//! It also exists to say *no*. Loose goods priced `[Per Kg]` must never be
//! compared against a fixed pack: 8.995 per kilo and 1.250 per punnet are both
//! real prices for the same food and neither tells you anything about the other.

/// What a quantity measures. Comparing across dimensions is always wrong, so
/// this is checked before any number is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    Mass,
    Volume,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackSize {
    pub dimension: Dimension,
    /// Thousandths of the base unit — grams for mass, millilitres for volume,
    /// items for count. Thousandths because real labels carry `4.8 G` and
    /// `0.10OZ`, and integers keep the comparison exact.
    pub each_milli: i64,
    /// `6` in `6x200ml`. One for a single container.
    pub multiplier: i64,
    /// Priced by weight or volume rather than sold as a pack. Never comparable
    /// to a fixed pack, whatever the numbers say.
    pub loose: bool,
    /// Converted from ounces or pounds, so the figure is only as precise as the
    /// label's own rounding — `0.10 OZ` on a tube that actually holds 3g is out
    /// by 5.6%. Widening the tolerance for every size to absorb that would let
    /// genuinely different metric packs match, so it is carried here and applied
    /// only where it is earned.
    pub approximate: bool,
}

impl PackSize {
    pub fn total_milli(&self) -> i64 {
        self.each_milli.saturating_mul(self.multiplier)
    }

    /// Do two sizes describe the same thing?
    ///
    /// A tolerance rather than equality, because labels round. Two percent
    /// between metric figures is enough for that and nowhere near enough to let
    /// 200ml pass as 250ml. Where one side was converted from ounces the label's
    /// own rounding is already 5-6% out, so that comparison — and only that
    /// one — gets the wider window.
    pub fn comparable_to(&self, other: &PackSize) -> bool {
        if self.dimension != other.dimension || self.loose != other.loose {
            return false;
        }
        if self.loose {
            return true; // both priced per unit weight: the rate is the price
        }
        let (a, b) = (self.total_milli(), other.total_milli());
        if a <= 0 || b <= 0 {
            return false;
        }
        let denominator = if self.approximate || other.approximate {
            12
        } else {
            50
        };
        (a - b).abs() * denominator <= a.max(b)
    }

    /// Used by the Market panel, which is not built yet — the service and
    /// commands are. Narrow rather than module-wide so the rest of this file
    /// still reports honestly.
    #[allow(dead_code)]
    pub fn describe(&self) -> String {
        if self.loose {
            return match self.dimension {
                Dimension::Mass => "per kg".into(),
                Dimension::Volume => "per litre".into(),
                Dimension::Count => "per item".into(),
            };
        }
        let unit = match self.dimension {
            Dimension::Mass => "g",
            Dimension::Volume => "ml",
            Dimension::Count => "pc",
        };
        let each = format_milli(self.each_milli);
        if self.multiplier > 1 {
            format!("{}x{}{}", self.multiplier, each, unit)
        } else {
            format!("{each}{unit}")
        }
    }
}

#[allow(dead_code)]
fn format_milli(milli: i64) -> String {
    if milli % 1000 == 0 {
        return (milli / 1000).to_string();
    }
    format!("{}", milli as f64 / 1000.0)
}

/// Base unit and scale for a unit token, or None if it is not a unit at all.
fn unit_of(token: &str) -> Option<(Dimension, i64)> {
    // Scale converts one of this unit into thousandths of the base unit.
    Some(match token {
        "mg" => (Dimension::Mass, 1),
        "g" | "gm" | "gms" | "gr" | "gram" | "grams" => (Dimension::Mass, 1_000),
        "kg" | "kgs" | "kilo" | "kilos" => (Dimension::Mass, 1_000_000),
        // Avoirdupois ounce and pound, which appear on imported personal care.
        "oz" => (Dimension::Mass, 28_349),
        "lb" | "lbs" => (Dimension::Mass, 453_592),
        "ml" | "mls" => (Dimension::Volume, 1_000),
        "cl" => (Dimension::Volume, 10_000),
        "l" | "ltr" | "ltrs" | "litre" | "litres" | "liter" | "liters" => {
            (Dimension::Volume, 1_000_000)
        }
        "pc" | "pcs" | "piece" | "pieces" | "s" | "tabs" | "caps" | "sheets" | "wipes" => {
            (Dimension::Count, 1_000)
        }
        _ => return None,
    })
}

/// One number-and-unit pair found in a string.
struct Measure {
    dimension: Dimension,
    milli: i64,
    metric: bool,
}

/// Read a size out of any text — a dedicated size line or a product name.
///
/// Returns None rather than guessing. A product whose size cannot be read is a
/// product that can only be matched on name, which the trust model already
/// handles by refusing to trust it without an operator saying so.
pub fn parse_pack(text: &str) -> Option<PackSize> {
    let lower = text.to_lowercase();
    if lower.is_empty() {
        return None;
    }
    if is_loose(&lower) {
        return Some(PackSize {
            dimension: loose_dimension(&lower),
            each_milli: 1_000_000,
            multiplier: 1,
            loose: true,
            approximate: false,
        });
    }

    let measures = scan_measures(&lower);
    let chosen = choose_measure(&measures)?;
    let multiplier = leading_multiplier(&lower).unwrap_or(1);
    Some(PackSize {
        dimension: chosen.dimension,
        each_milli: chosen.milli,
        multiplier,
        loose: false,
        approximate: !chosen.metric,
    })
}

fn is_loose(lower: &str) -> bool {
    [
        "per kg",
        "per kilo",
        "/kg",
        "per litre",
        "per liter",
        "/l ",
        "per piece",
        "per pc",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn loose_dimension(lower: &str) -> Dimension {
    if lower.contains("litre") || lower.contains("liter") || lower.contains("/l") {
        Dimension::Volume
    } else if lower.contains("piece") || lower.contains("per pc") {
        Dimension::Count
    } else {
        Dimension::Mass
    }
}

/// `6x200ml` and `4 × 90g`. Only counts when the multiplier sits before the
/// measurement, so `500g x 2 pack` is left at one rather than read backwards.
fn leading_multiplier(lower: &str) -> Option<i64> {
    let bytes: Vec<char> = lower.chars().collect();
    for (index, ch) in bytes.iter().enumerate() {
        if *ch != 'x' && *ch != '*' && *ch != '\u{d7}' {
            continue;
        }
        // "4 × 90g" spaces the multiplier off the sign, so step over the gap
        // before looking for its digits.
        let mut start = index;
        while start > 0 && bytes[start - 1].is_whitespace() {
            start -= 1;
        }
        let digits_end = start;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start == digits_end {
            continue;
        }
        // Something numeric has to follow, or this is the "x" in a brand name.
        let follows_number = bytes[index + 1..]
            .iter()
            .find(|c| !c.is_whitespace())
            .is_some_and(|c| c.is_ascii_digit());
        if !follows_number {
            continue;
        }
        let value: i64 = bytes[start..digits_end]
            .iter()
            .collect::<String>()
            .parse()
            .ok()?;
        if (2..=144).contains(&value) {
            return Some(value);
        }
    }
    None
}

fn scan_measures(lower: &str) -> Vec<Measure> {
    let chars: Vec<char> = lower.chars().collect();
    let mut measures = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
            index += 1;
        }
        let number: String = chars[start..index].iter().collect();
        let mut cursor = index;
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        let unit_start = cursor;
        while cursor < chars.len() && chars[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        if unit_start == cursor {
            continue;
        }
        let token: String = chars[unit_start..cursor].iter().collect();
        let Some((dimension, scale)) = unit_of(&token) else {
            continue;
        };
        let Some(milli) = scale_amount(&number, scale) else {
            continue;
        };
        measures.push(Measure {
            dimension,
            milli,
            metric: !matches!(token.as_str(), "oz" | "lb" | "lbs"),
        });
        index = cursor;
    }
    measures
}

/// Multiply a decimal string by a scale without floating point — `4.8` and a
/// scale of 1000 has to be exactly 4800, not 4799.999999.
fn scale_amount(number: &str, scale: i64) -> Option<i64> {
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let mut value = whole.checked_mul(scale)?;
    let mut step = scale;
    for digit in fraction.chars() {
        step /= 10;
        if step == 0 {
            break;
        }
        value = value.checked_add(step.checked_mul(digit.to_digit(10)? as i64)?)?;
    }
    (value > 0).then_some(value)
}

/// Which measurement in a name is the pack size?
///
/// `VASELINE LIP CARE KISSING RED 0.10OZ 3 G` states the same tube twice. The
/// metric figure is the declared one on every label sold here, so it wins; and
/// where several metric figures appear, the last is the one at the end of the
/// name where sizes go.
fn choose_measure(measures: &[Measure]) -> Option<&Measure> {
    measures
        .iter()
        .rev()
        .find(|measure| measure.metric)
        .or_else(|| measures.last())
}

#[cfg(test)]
mod tests;
