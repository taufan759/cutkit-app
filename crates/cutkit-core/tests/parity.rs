//! Memastikan Rust menghasilkan nilai yang sama dengan `tests/fixtures/parity.json`.
//! Tes JS (`tests/parity.test.js`) memeriksa fixture yang sama untuk sisi preview.

use cutkit_core::ass::{cues, paginate};
use cutkit_core::layout::place;
use cutkit_core::{Fit, Motion, Transform, Word};
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/parity.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn pagination_matches_fixture() {
    for case in fixture()["pagination"].as_array().unwrap() {
        let words: Vec<Word> = case["words"].as_array().unwrap().iter().map(|w| Word { text: w[0].as_str().unwrap().into(), start: f(&w[1]), end: f(&w[2]) }).collect();
        let pages = paginate(&words, case["words_per_page"].as_u64().unwrap() as usize, case["max_line_chars"].as_u64().unwrap() as usize);
        let got: Vec<Vec<Vec<usize>>> = pages.iter().map(|p| p.lines.clone()).collect();
        let want: Vec<Vec<Vec<usize>>> = serde_json::from_value(case["pages"].clone()).unwrap();
        assert_eq!(got, want);
        let cs = cues(&words, &pages);
        let want_cues = case["cues"].as_array().unwrap();
        assert_eq!(cs.len(), want_cues.len());
        for (c, w) in cs.iter().zip(want_cues) {
            assert_eq!(c.page, w[0].as_u64().unwrap() as usize);
            assert_eq!(c.active, w[1].as_u64().unwrap() as usize);
            assert!((c.start - f(&w[2])).abs() < 1e-9 && (c.end - f(&w[3])).abs() < 1e-9, "{c:?} vs {w:?}");
        }
    }
}

#[test]
fn motion_matches_fixture() {
    for case in fixture()["motion"].as_array().unwrap() {
        let m = Motion { kind: case["kind"].as_str().unwrap().into(), amount: f(&case["amount"]) };
        let (z, px, py) = m.at(f(&case["u"]));
        let want = case["out"].as_array().unwrap();
        for (got, w) in [z, px, py].iter().zip(want) {
            assert!((got - f(w)).abs() < 1e-9, "kasus {case}: {z} {px} {py}");
        }
    }
}

#[test]
fn placement_matches_fixture() {
    for case in fixture()["placement"].as_array().unwrap() {
        let t = &case["t"];
        let tr = Transform { x: f(&t[0]), y: f(&t[1]), scale: f(&t[2]), rotation: f(&t[3]), opacity: f(&t[4]) };
        let fit = if case["fit"] == "cover" { Fit::Cover } else { Fit::Contain };
        let p = place(
            case["canvas"][0].as_u64().unwrap() as u32,
            case["canvas"][1].as_u64().unwrap() as u32,
            case["asset"][0].as_u64().unwrap() as u32,
            case["asset"][1].as_u64().unwrap() as u32,
            fit,
            &tr,
        );
        let got = vec![p.w as i64, p.h as i64, p.ow as i64, p.oh as i64, p.x, p.y];
        let want: Vec<i64> = serde_json::from_value(case["out"].clone()).unwrap();
        assert_eq!(got, want, "kasus {case}");
    }
}
