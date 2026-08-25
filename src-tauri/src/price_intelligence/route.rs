//! Deciding which sources are worth asking about a product.
//!
//! Asking every source about everything is slow for the operator and rude to
//! the sources: a phone looked up in a grocery aggregator is a request that
//! could never have returned anything. So a product is routed first, and only
//! sources covering one of its routes are queried.
//!
//! Routes are inferred, never required. A shop's own category is good evidence
//! and is weighted accordingly, but a product filed under "Misc" — or under
//! nothing at all — still routes on its name, because the alternative is a
//! feature that only works once the catalogue has been tidied.
//!
//! `general` is always present at a low score. It is what guarantees at least
//! one source is asked, so an unrecognised product falls back to a broad search
//! rather than silently returning "no sources".

use std::collections::BTreeMap;

/// A route the product plausibly belongs to, scored 0-100.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub name: &'static str,
    pub score: u8,
}

/// The shop's own filing is stronger evidence than a word in a product name:
/// someone put it there deliberately, and "Cream" appears in both a face cream
/// and a carton of it.
const CATEGORY_WEIGHT: u16 = 60;
const NAME_WEIGHT: u16 = 34;

/// Below this a route is noise — one incidental word in a long name.
const MIN_SCORE: u8 = 20;

/// Always-on floor, so every product reaches at least the broad sources.
const GENERAL_FLOOR: u8 = 25;

type Keywords = (&'static str, &'static [&'static str]);

/// Arabic terms are carried alongside the English because the catalogue holds
/// both, exactly as the AI tool-subsetting keyword table does.
const ROUTES: &[Keywords] = &[
    (
        "food",
        &[
            "milk", "laban", "cheese", "yoghurt", "yogurt", "bread", "rice", "flour", "sugar",
            "oil", "tuna", "sardine", "chicken", "meat", "beef", "lamb", "fish", "egg", "pasta",
            "noodle", "biscuit", "chocolate", "snack", "crisps", "chips", "date", "honey", "jam",
            "sauce", "ketchup", "spice", "tahina", "hummus", "frozen", "grocery", "cereal",
            "حليب", "لبن", "جبن", "خبز", "أرز", "دقيق", "سكر", "زيت", "دجاج", "لحم", "سمك", "بيض",
        ],
    ),
    (
        "beverages",
        &[
            "water", "juice", "cola", "pepsi", "sprite", "7up", "soda", "drink", "beverage",
            "coffee", "tea", "energy drink", "squash", "syrup",
            "ماء", "عصير", "مشروب", "قهوة", "شاي",
        ],
    ),
    (
        "cosmetics",
        &[
            "lipstick", "mascara", "kajal", "eyeliner", "foundation", "concealer", "blush",
            "makeup", "make-up", "nail polish", "compact", "primer", "highlighter", "palette",
            "مكياج", "أحمر شفاه", "كحل",
        ],
    ),
    (
        "personal_care",
        &[
            "shampoo", "conditioner", "soap", "body wash", "shower gel", "deodorant", "antiperspirant",
            "toothpaste", "toothbrush", "mouthwash", "lotion", "moisturiser", "moisturizer",
            "cream", "serum", "lip care", "lip balm", "petroleum jelly", "vaseline", "razor",
            "shaving", "talc", "hand cream", "sunscreen", "face wash", "cleanser", "perfume",
            "fragrance", "cologne", "edt", "edp", "tissue", "sanitary", "hair oil", "hair dye",
            "شامبو", "صابون", "معجون أسنان", "كريم", "عطر", "مزيل عرق",
        ],
    ),
    (
        "baby",
        &[
            "baby", "infant", "diaper", "nappy", "pampers", "wipes", "formula", "teether",
            "pacifier", "johnson", "baby soap", "baby oil",
            "أطفال", "حفاضات", "رضاعة",
        ],
    ),
    (
        "household",
        &[
            "detergent", "bleach", "clorox", "dishwash", "cleaner", "disinfectant", "air freshener",
            "fabric softener", "laundry", "toilet", "kitchen roll", "foil", "cling film",
            "bin bag", "garbage bag", "insecticide", "mop", "sponge",
            "منظف", "مبيض", "غسيل", "مطهر",
        ],
    ),
    (
        "health",
        &[
            "vitamin", "supplement", "tablet", "capsule", "syrup", "plaster", "bandage",
            "thermometer", "sanitiser", "sanitizer", "mask", "first aid", "pharmacy",
            "فيتامين", "دواء", "ضمادة",
        ],
    ),
    (
        "electronics",
        &[
            "phone", "smartphone", "iphone", "galaxy", "laptop", "tablet pc", "television", "tv",
            "headphone", "earbuds", "charger", "power bank", "cable", "usb", "router", "camera",
            "console", "playstation", "xbox", "washing machine", "refrigerator", "microwave",
            "air conditioner", "vacuum cleaner", "kettle", "blender", "gb", "tb",
            "هاتف", "جوال", "لابتوب", "تلفزيون", "شاحن",
        ],
    ),
];

/// Score a product into routes, best first.
pub fn routes_for(product_name: &str, category_name: Option<&str>) -> Vec<Route> {
    let name = product_name.to_lowercase();
    let category = category_name.unwrap_or_default().to_lowercase();

    let mut scores: BTreeMap<&'static str, u16> = BTreeMap::new();
    for (route, keywords) in ROUTES {
        let mut score = 0_u16;
        for keyword in *keywords {
            if !category.is_empty() && category.contains(keyword) {
                score += CATEGORY_WEIGHT;
            }
            if name.contains(keyword) {
                score += NAME_WEIGHT;
            }
        }
        if score > 0 {
            scores.insert(route, score);
        }
    }

    let mut routes: Vec<Route> = scores
        .into_iter()
        .map(|(name, score)| Route {
            name,
            score: score.min(100) as u8,
        })
        .filter(|route| route.score >= MIN_SCORE)
        .collect();

    // Electronics and groceries share no shelf and no supplier. When a name
    // says "Galaxy S26 256GB" loudly enough, keeping a stray grocery route
    // alive only spends a request on a source that cannot answer.
    if let Some(top) = routes.iter().map(|route| route.score).max() {
        if top >= 60 {
            routes.retain(|route| route.score * 2 >= top);
        }
    }

    routes.sort_by(|a, b| b.score.cmp(&a.score).then(a.name.cmp(b.name)));
    routes.push(Route {
        name: "general",
        score: GENERAL_FLOOR,
    });
    routes
}

/// Which registered sources cover any of these routes.
///
/// Order follows the routes, so the source matching the strongest signal is
/// asked first and a caller that only wants one answer gets the best one.
pub fn sources_for<'a>(
    routes: &[Route],
    registered: &'a [(String, Vec<String>)],
) -> Vec<&'a str> {
    let mut chosen: Vec<&str> = Vec::new();
    for route in routes {
        for (source_id, covers) in registered {
            if covers.iter().any(|c| c == route.name) && !chosen.contains(&source_id.as_str()) {
                chosen.push(source_id);
            }
        }
    }
    chosen
}

#[cfg(test)]
mod tests;
