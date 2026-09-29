//! HUNTDAT\_RES.TXT: the weapons, creatures and prices of Carnivores 2 and
//! Ice Age. Keys are matched the way the original
//! matched them - by substring - so "hearK" counts as "hear".

use super::DinoInfo;

#[derive(Clone, Debug, Default)]
pub struct WeapInfo {
    pub name: String,
    pub file: String,
    pub pic: String,
    pub power: f32,
    pub prec: f32,
    pub loud: f32,
    pub rate: f32,
    pub shots: i32,
    pub reload: i32,
    pub trace_c: i32,
    pub optic: i32,
    pub fall: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub weapons: Vec<WeapInfo>,
    pub dinos: Vec<DinoInfo>,
    pub start_credits: i32,
    pub area_prices: Vec<i32>,
    pub dino_prices: Vec<i32>,
    pub weapon_prices: Vec<i32>,
    pub access_prices: Vec<i32>,
    /// Whether there was a price table at all.
    pub has_prices: bool,
}

fn quoted(line: &str) -> Option<String> {
    let a = line.find('\'')?;
    let rest = &line[a + 1..];
    let b = rest.find('\'').unwrap_or(rest.len());
    Some(rest[..b].to_string())
}

fn value(line: &str) -> &str {
    line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("")
}

fn atof(v: &str) -> f32 {
    let end = v
        .char_indices()
        .find(|&(i, ch)| {
            !(ch.is_ascii_digit() || ch == '.' || ((ch == '-' || ch == '+') && i == 0))
        })
        .map(|(i, _)| i)
        .unwrap_or(v.len());
    v[..end].parse().unwrap_or(0.0)
}

fn atoi(v: &str) -> i32 {
    atof(v) as i32
}

pub fn parse(text: &str) -> Script {
    let mut s = Script::default();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.starts_with('.') {
            break;
        }
        if line.contains("weapons") {
            read_blocks(&mut lines, |block| s.weapons.push(weapon(block)));
        } else if line.contains("characters") {
            read_blocks(&mut lines, |block| s.dinos.push(dino(block)));
        } else if line.contains("prices") {
            s.has_prices = true;
            for l in lines.by_ref() {
                if l.contains('}') {
                    break;
                }
                let v = atoi(value(l));
                if l.contains("start") {
                    s.start_credits = v;
                } else if l.contains("area") {
                    s.area_prices.push(v);
                } else if l.contains("dino") {
                    s.dino_prices.push(v);
                } else if l.contains("weapon") {
                    s.weapon_prices.push(v);
                } else if l.contains("acces") {
                    s.access_prices.push(v);
                }
            }
        }
    }
    s
}

/// Reads `{ ... }` blocks until the closing brace of the list.
fn read_blocks<'a>(lines: &mut impl Iterator<Item = &'a str>, mut f: impl FnMut(&[&'a str])) {
    while let Some(line) = lines.next() {
        if line.contains('}') {
            break;
        }
        if line.contains('{') {
            let mut block = Vec::new();
            for l in lines.by_ref() {
                if l.contains('}') {
                    break;
                }
                if l.contains('=') {
                    block.push(l);
                }
            }
            f(&block);
        }
    }
}

fn weapon(block: &[&str]) -> WeapInfo {
    let mut w = WeapInfo::default();
    for &l in block {
        let v = value(l);
        if l.contains("power") {
            w.power = atof(v);
        }
        if l.contains("prec") {
            w.prec = atof(v);
        }
        if l.contains("loud") {
            w.loud = atof(v);
        }
        if l.contains("rate") {
            w.rate = atof(v);
        }
        if l.contains("shots") {
            w.shots = atoi(v);
        }
        if l.contains("reload") {
            w.reload = atoi(v);
        }
        if l.contains("trace") {
            w.trace_c = atoi(v) - 1;
        }
        if l.contains("optic") {
            w.optic = atoi(v);
        }
        // The community's extended engine marks a scope with its
        // magnification instead; any above 1 is the stock 3x optic.
        if l.contains("zoom") && atof(v) > 1.0 {
            w.optic = 1;
        }
        if l.contains("fall") {
            w.fall = atoi(v);
        }
        if l.contains("name") {
            w.name = quoted(l).unwrap_or_default();
        }
        if l.contains("file") {
            w.file = quoted(l).unwrap_or_default();
        }
        if l.contains("pic") {
            w.pic = quoted(l).unwrap_or_default();
        }
    }
    w
}

fn dino(block: &[&str]) -> DinoInfo {
    let mut d = DinoInfo::default();
    for &l in block {
        let v = value(l);
        // The key is what comes before '=': the original matched the whole
        // line, which only ever mattered for the name and file values.
        let key = l.split_once('=').map(|(k, _)| k).unwrap_or(l);
        if key.contains("mass") {
            d.mass = atof(v);
        }
        if key.contains("length") {
            d.length = atof(v);
        }
        if key.contains("radius") {
            d.radius = atof(v);
        }
        if key.contains("health") {
            d.health0 = atoi(v);
        }
        if key.contains("basescore") {
            d.base_score = atoi(v);
        }
        if key.contains("ai") {
            d.ai = atoi(v);
        }
        if key.contains("smell") {
            d.smell_k = atof(v);
        }
        if key.contains("hear") {
            d.hear_k = atof(v);
        }
        if key.contains("look") {
            d.look_k = atof(v);
        }
        if key.contains("shipdelta") {
            d.sh_delta = atof(v);
        }
        if key.contains("scale0") {
            d.scale0 = atoi(v);
        }
        if key.contains("scaleA") {
            d.scale_a = atoi(v);
        }
        if key.contains("danger") {
            d.danger = true;
        }
        if key.contains("name") {
            d.name = quoted(l).unwrap_or_default();
        }
        if key.contains("file") {
            d.file = quoted(l).unwrap_or_default();
        }
        if key.contains("pic") {
            d.pic = quoted(l).unwrap_or_default();
        }
    }
    d.set_retail_layout();
    d
}
