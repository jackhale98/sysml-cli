//! Editor language features (completion, hover, definition, references,
//! rename, outline, formatting, quick fixes) against the SysML Web demo
//! model, through the JSON entry points the browser calls.
//!
//! Uses `fixtures/drone`, the copy of the demo shared with `drone_views`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{json, Value};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "sysml") {
            out.push(path);
        }
    }
}

/// The demo project as `(path, text)` pairs.
fn drone() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drone");
    let mut paths = Vec::new();
    collect(&root, &mut paths);
    paths.sort();
    paths
        .iter()
        .map(|p| {
            (
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                std::fs::read_to_string(p).unwrap(),
            )
        })
        .collect()
}

fn to_json(files: &[(String, String)]) -> String {
    let list: Vec<Value> = files
        .iter()
        .map(|(path, text)| json!({ "path": path, "text": text }))
        .collect();
    serde_json::to_string(&list).unwrap()
}

fn text<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
    &files.iter().find(|(p, _)| p == path).unwrap().1
}

/// UTF-16 offset of the `nth` occurrence of `needle` (plus `delta` units).
fn offset(text: &str, needle: &str, nth: usize, delta: u32) -> u32 {
    let byte = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} #{nth} not found"))
        .0;
    text[..byte].encode_utf16().count() as u32 + delta
}

fn parse(out: Result<String, wasm_bindgen::JsError>) -> Value {
    serde_json::from_str(&out.unwrap_or_else(|_| panic!("call failed"))).unwrap()
}

/// Apply `[{ path, from, to, insert }]` changes to the files.
fn apply(files: &[(String, String)], changes: &Value) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| {
            let mut units: Vec<u16> = text.encode_utf16().collect();
            let mut mine: Vec<&Value> = changes
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["path"] == path.as_str())
                .collect();
            mine.sort_by_key(|c| std::cmp::Reverse(c["from"].as_u64().unwrap()));
            for c in mine {
                let (from, to) = (
                    c["from"].as_u64().unwrap() as usize,
                    c["to"].as_u64().unwrap() as usize,
                );
                let insert: Vec<u16> = c["insert"].as_str().unwrap().encode_utf16().collect();
                units.splice(from..to, insert);
            }
            (path.clone(), String::from_utf16(&units).unwrap())
        })
        .collect()
}

const ARCH: &str = "system/DroneArchitecture.sysml";
const COMPONENTS: &str = "system/Components.sysml";

fn diagnostic_codes(files: &[(String, String)]) -> Vec<String> {
    let out = parse(sysml_wasm::analyze_project(&to_json(files)));
    let mut codes: Vec<String> = out
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|a| a["diagnostics"].as_array().unwrap().clone())
        .map(|d| format!("{} {}", d["code"], d["message"]))
        .collect();
    codes.sort();
    codes
}

#[test]
fn completion_after_a_colon_offers_the_component_types() {
    let mut files = drone();
    let arch = files.iter_mut().find(|(p, _)| p == ARCH).unwrap();
    arch.1 = arch
        .1
        .replace("part battery : Battery;", "part battery : ;");
    let at = offset(&arch.1, "part battery : ;", 0, 15);
    let out = parse(sysml_wasm::language::complete(&to_json(&files), ARCH, at));
    assert_eq!(
        (out["from"].as_u64(), out["to"].as_u64()),
        (Some(at as u64), Some(at as u64))
    );
    let items = out["items"].as_array().unwrap();
    let battery = items
        .iter()
        .find(|i| i["label"] == "Battery")
        .expect("Battery offered");
    assert_eq!(battery["kind"], "definition");
    assert!(battery["detail"]
        .as_str()
        .unwrap()
        .contains("DroneComponents"));
    assert!(battery["documentation"]
        .as_str()
        .unwrap()
        .contains("Six-cell"));
    // Library types too, ranked by the editor through `kind`.
    let real = items.iter().find(|i| i["label"] == "Real").expect("Real");
    assert_eq!(real["kind"], "library");

    // With a prefix the list is filtered and the range covers the word.
    let mut files = drone();
    let arch = files.iter_mut().find(|(p, _)| p == ARCH).unwrap();
    arch.1 = arch
        .1
        .replace("part battery : Battery;", "part battery : Batt;");
    let at = offset(&arch.1, "Batt;", 0, 4);
    let out = parse(sysml_wasm::language::complete(&to_json(&files), ARCH, at));
    assert_eq!(out["from"], at - 4);
    let labels: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"Battery"), "{labels:?}");
    assert!(!labels.contains(&"Airframe"), "{labels:?}");
}

#[test]
fn completion_of_members_imports_and_statements() {
    let files = drone();
    let json = to_json(&files);
    let arch = text(&files, ARCH);

    // `battery.` lists the members of Battery, inherited ones included.
    let at = offset(arch, "battery.usableEnergy", 0, 8);
    let out = parse(sysml_wasm::language::complete(&json, ARCH, at));
    let labels: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    for m in ["usableEnergy", "capacity", "powerOut", "mass"] {
        assert!(labels.contains(&m), "{m} in {labels:?}");
    }

    // `import DroneComponents::` offers members and `*`.
    let at = offset(arch, "DroneComponents::*", 0, 17);
    let out = parse(sysml_wasm::language::complete(&json, ARCH, at));
    let labels: Vec<&str> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(
        labels.contains(&"*") && labels.contains(&"Battery"),
        "{labels:?}"
    );

    // At the start of a line in a body: keywords and snippets.
    let at = offset(arch, "        part airframe", 0, 8);
    let out = parse(sysml_wasm::language::complete(&json, ARCH, at));
    let items = out["items"].as_array().unwrap();
    let snippet = items.iter().find(|i| i["label"] == "part def").unwrap();
    assert_eq!(snippet["kind"], "snippet");
    assert_eq!(snippet["isSnippet"], true);
    assert_eq!(snippet["insertText"], "part def ${1:Name} {\n\t$0\n}");
    assert!(items
        .iter()
        .any(|i| i["kind"] == "keyword" && i["label"] == "attribute"));
}

#[test]
fn hover_shows_the_declaration_and_doc() {
    let files = drone();
    let json = to_json(&files);
    let arch = text(&files, ARCH);
    let at = offset(arch, "Battery;", 0, 2);
    let out = parse(sysml_wasm::language::hover(&json, ARCH, at));
    let md = out["markdown"].as_str().unwrap();
    assert!(md.contains("part def Battery :> Component"), "{md}");
    assert!(md.contains("`DroneComponents::Battery`"), "{md}");
    assert!(md.contains("Six-cell lithium-ion pack"), "{md}");
    assert_eq!(out["from"], at - 2);
    assert_eq!(out["to"], at + 5);

    // A library quantity: `ISQ::mass` (ISQBase, re-exported by ISQ).
    let comps = text(&files, COMPONENTS);
    let at = offset(comps, "ISQ::mass", 0, 6);
    let out = parse(sysml_wasm::language::hover(&json, COMPONENTS, at));
    let md = out["markdown"].as_str().unwrap();
    assert!(md.contains("attribute mass"), "{md}");
    assert!(md.contains("standard library"), "{md}");
    assert!(md.contains("MassValue"), "{md}");

    // Nothing to say about a keyword.
    let out = parse(sysml_wasm::language::hover(&json, ARCH, 1));
    assert!(out.is_null());
}

#[test]
fn definition_crosses_files_and_into_the_library() {
    let files = drone();
    let json = to_json(&files);
    let arch = text(&files, ARCH);
    let comps = text(&files, COMPONENTS);
    let out = parse(sysml_wasm::language::definition(
        &json,
        ARCH,
        offset(arch, "Battery;", 0, 0),
    ));
    let def = offset(comps, "Battery :> Component", 0, 0);
    assert_eq!(
        out,
        json!({ "path": COMPONENTS, "from": def, "to": def + 7 })
    );

    // A feature chain: `battery.usableEnergy`.
    let out = parse(sysml_wasm::language::definition(
        &json,
        ARCH,
        offset(arch, "usableEnergy", 0, 3),
    ));
    assert_eq!(out["path"], COMPONENTS);
    assert_eq!(out["from"], offset(comps, "usableEnergy", 0, 0));

    let out = parse(sysml_wasm::language::definition(
        &json,
        COMPONENTS,
        offset(comps, "Real;", 0, 0),
    ));
    let library = &out["library"];
    let path = library["path"].as_str().unwrap();
    assert!(path.ends_with("ScalarValues.kerml"), "{out}");
    let file: Value =
        serde_json::from_str(&sysml_wasm::library::library_file(path).ok().unwrap()).unwrap();
    let line = library["line"].as_u64().unwrap() as usize;
    assert!(file["text"]
        .as_str()
        .unwrap()
        .lines()
        .nth(line - 1)
        .unwrap()
        .contains("datatype Real"));
}

#[test]
fn references_span_files() {
    let files = drone();
    let json = to_json(&files);
    let comps = text(&files, COMPONENTS);
    // Component's `mass`: redefined by every component, used in chains.
    let at = offset(comps, "mass :> ISQ::mass", 0, 0);
    let out = parse(sysml_wasm::language::references(&json, COMPONENTS, at));
    let refs = out.as_array().unwrap();
    let in_arch = refs.iter().filter(|r| r["path"] == ARCH).count();
    let in_comps = refs.iter().filter(|r| r["path"] == COMPONENTS).count();
    assert!(in_comps >= 12, "declaration and redefinitions: {in_comps}");
    assert!(in_arch >= 9, "`battery.mass` and friends: {in_arch}");
    for r in refs {
        let t = text(&files, r["path"].as_str().unwrap());
        let units: Vec<u16> = t.encode_utf16().collect();
        let (f, e) = (
            r["from"].as_u64().unwrap() as usize,
            r["to"].as_u64().unwrap() as usize,
        );
        assert_eq!(String::from_utf16(&units[f..e]).unwrap(), "mass");
    }
}

#[test]
fn renaming_battery_updates_every_file() {
    let files = drone();
    let json = to_json(&files);
    let comps = text(&files, COMPONENTS);
    let before = diagnostic_codes(&files);
    let out = parse(sysml_wasm::language::rename_symbol(
        &json,
        COMPONENTS,
        offset(comps, "Battery :> Component", 0, 1),
        "Battery pack",
    ));
    let changes = &out["changes"];
    let paths: std::collections::BTreeSet<&str> = changes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert!(
        paths.contains(COMPONENTS) && paths.contains(ARCH),
        "{paths:?}"
    );
    let renamed = apply(&files, changes);
    assert!(text(&renamed, COMPONENTS).contains("part def 'Battery pack' :> Component"));
    assert!(text(&renamed, ARCH).contains("part battery : 'Battery pack';"));
    // `'Battery protection'` and the `battery` usage are other elements.
    assert!(
        text(&renamed, "requirements/SystemRequirements.sysml").contains("'Battery protection'")
    );
    assert!(text(&renamed, ARCH).contains("battery.usableEnergy"));
    let after: Vec<String> = diagnostic_codes(&renamed)
        .iter()
        .map(|d| d.replace("'Battery pack'", "Battery"))
        .collect();
    assert_eq!(after, before);

    // A clash is refused.
    let out = parse(sysml_wasm::language::rename_symbol(
        &json,
        COMPONENTS,
        offset(comps, "Battery :> Component", 0, 1),
        "Motor",
    ));
    assert!(out["error"].as_str().unwrap().contains("Motor"));
}

#[test]
fn renaming_a_usage_updates_its_feature_chains() {
    let files = drone();
    let json = to_json(&files);
    let arch = text(&files, ARCH);
    let before = diagnostic_codes(&files);
    let out = parse(sysml_wasm::language::rename_symbol(
        &json,
        ARCH,
        offset(arch, "part battery", 0, 5),
        "mainBattery",
    ));
    let renamed = apply(&files, &out["changes"]);
    let arch2 = text(&renamed, ARCH);
    assert!(arch2.contains("part mainBattery : Battery;"));
    assert!(arch2.contains("connect mainBattery.powerOut"));
    assert!(arch2.contains("HoverEndurance(mainBattery.usableEnergy"));
    assert!(text(&renamed, "system/Satisfaction.sysml").contains("drone.mainBattery"));
    assert_eq!(diagnostic_codes(&renamed), before);
}

#[test]
fn outline_of_the_components() {
    let files = drone();
    let out = parse(sysml_wasm::language::outline(&to_json(&files), COMPONENTS));
    let pkg = &out[0];
    assert_eq!(pkg["name"], "DroneComponents");
    assert_eq!(pkg["kind"], "package");
    let battery = pkg["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Battery")
        .unwrap();
    assert_eq!(battery["kind"], "part def");
    assert_eq!(battery["detail"], "Component");
    let comps = text(&files, COMPONENTS);
    assert_eq!(
        battery["selectionFrom"],
        offset(comps, "Battery :> Component", 0, 0)
    );
    let members: Vec<&str> = battery["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        members,
        [
            "mass",
            "nominalVoltage",
            "capacity",
            "usableFraction",
            "usableEnergy",
            "powerOut"
        ]
    );
}

#[test]
fn formatting_the_demo_changes_nothing_semantic() {
    for (path, source) in drone() {
        let out: Value =
            serde_json::from_str(&sysml_wasm::language::format_source(&source)).unwrap();
        let changes: Vec<Value> = out["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c["path"] = json!(path);
                c
            })
            .collect();
        let formatted = apply(&[(path.clone(), source.clone())], &json!(changes));
        let again: Value =
            serde_json::from_str(&sysml_wasm::language::format_source(&formatted[0].1)).unwrap();
        assert_eq!(again["changes"], json!([]), "{path} formats idempotently");
        let strip = |s: &str| s.split_whitespace().collect::<String>();
        assert_eq!(strip(&formatted[0].1), strip(&source), "{path}");
    }
}

#[test]
fn quick_fix_adds_the_missing_import() {
    let mut files = drone();
    let arch = files.iter_mut().find(|(p, _)| p == ARCH).unwrap();
    arch.1 = arch
        .1
        .replace("    private import DroneComponents::*;\n", "");
    let json = to_json(&files);
    let arch = text(&files, ARCH);
    let at = offset(arch, "Battery;", 0, 0);
    let out = parse(sysml_wasm::language::code_actions(&json, ARCH, at, at));
    let actions = out.as_array().unwrap();
    let import = actions
        .iter()
        .find(|a| a["title"] == "Import `DroneComponents::Battery`")
        .unwrap_or_else(|| panic!("{out:#}"));
    assert_eq!(import["diagnosticCode"], "W004");
    let fixed = apply(&files, &import["changes"]);
    assert!(text(&fixed, ARCH).contains(
        "    private import DronePerformance::HoverEndurance;\n    private import DroneComponents::Battery;\n"
    ));
    let w004 = |files: &[(String, String)]| {
        diagnostic_codes(files)
            .into_iter()
            .filter(|d| d.starts_with("\"W004\"") && d.contains("`Battery`"))
            .count()
    };
    assert_eq!(w004(&files), 1);
    assert_eq!(w004(&fixed), 0);

    // A typo gets the near names.
    let mut files = drone();
    let arch = files.iter_mut().find(|(p, _)| p == ARCH).unwrap();
    arch.1 = arch
        .1
        .replace("part battery : Battery;", "part battery : Batery;");
    let arch_text = arch.1.clone();
    let at = offset(&arch_text, "Batery", 0, 2);
    let out = parse(sysml_wasm::language::code_actions(
        &to_json(&files),
        ARCH,
        at,
        at,
    ));
    let fix = out
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == "Change to `Battery`")
        .unwrap_or_else(|| panic!("{out:#}"));
    assert!(text(&apply(&files, &fix["changes"]), ARCH).contains("part battery : Battery;"));
}

/// Latency on the demo: `cargo test --release -p sysml-wasm --test
/// drone_language latency -- --ignored --nocapture`.
#[test]
#[ignore]
fn latency() {
    let mut files = drone();
    let json = to_json(&files);
    let arch = text(&files, ARCH).to_owned();
    let at = offset(&arch, "Battery;", 0, 2);
    let time = |label: &str, n: u32, f: &mut dyn FnMut(u32)| {
        let start = Instant::now();
        for i in 0..n {
            f(i);
        }
        println!(
            "{label}: {:.2} ms",
            start.elapsed().as_secs_f64() * 1e3 / n as f64
        );
    };
    time("first call (index project + library)", 1, &mut |_| {
        sysml_wasm::language::hover(&json, ARCH, at).unwrap();
    });
    time("hover, unchanged project", 50, &mut |_| {
        sysml_wasm::language::hover(&json, ARCH, at).unwrap();
    });
    time("complete type position, unchanged project", 50, &mut |_| {
        sysml_wasm::language::complete(&json, ARCH, at).unwrap();
    });
    // Typing: the edited file changes on every call.
    let mut typing: Vec<(String, u32, u32)> = Vec::new();
    for i in 0..50 {
        let edited = arch.replace(
            "part battery : Battery;",
            &format!("part battery{i} : Batt;"),
        );
        let complete_at = offset(&edited, "Batt;", 0, 4);
        let hover_at = offset(&edited, "Airframe;", 0, 2);
        files.iter_mut().find(|(p, _)| p == ARCH).unwrap().1 = edited;
        typing.push((to_json(&files), complete_at, hover_at));
    }
    time(
        "complete while typing (one file re-indexed)",
        50,
        &mut |i| {
            let (json, at, _) = &typing[i as usize];
            sysml_wasm::language::complete(json, ARCH, *at).unwrap();
        },
    );
    time("hover while typing (one file re-indexed)", 50, &mut |i| {
        let (json, _, at) = &typing[(i as usize + 1) % 50];
        sysml_wasm::language::hover(json, ARCH, *at).unwrap();
    });
}
