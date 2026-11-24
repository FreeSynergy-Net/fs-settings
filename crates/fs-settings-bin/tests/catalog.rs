//! The module form of fs-settings (SB-6): `settings/catalog/commands.json` and `forms.json` have the shape the host reads (read.rs and
//! declared.rs of fs-starter-host), and the form `fields-in-groups@1.0.0` is mapped completely onto channels that the form has.
//! Pattern: Contract test (the files against the one table of forms in fs-interfaces).

use std::collections::BTreeSet;
use std::path::PathBuf;

use fs_interfaces::forms;
use serde_json::Value;

fn read(file: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../settings/catalog")
        .join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn command_keys() -> BTreeSet<String> {
    read("commands.json")["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|c| c["capability"].as_str().expect("capability").to_owned())
        .collect()
}

#[test]
fn commands_are_bare_keys_of_the_module_without_id() {
    let doc = read("commands.json");
    assert_eq!(doc["module"], "settings");
    assert_eq!(doc["contract"], "settings");
    assert_eq!(doc["scope"], "own-contract");
    let list = doc["commands"].as_array().expect("commands");
    assert_ne!(list.len(), 0);
    for c in list {
        let key = c["capability"].as_str().expect("capability");
        assert!(key.starts_with("value."), "{key}");
        assert!(c.get("id").is_none(), "{key}: the host derives the id");
        assert!(["read", "write", "delete"].contains(&c["access"].as_str().expect("access")));
        let help = &c["help"];
        for word in [&help["text"]["en"], &help["text"]["de"], &help["topic"]] {
            assert!(word.as_str().is_some_and(|t| !t.is_empty()), "{key}");
        }
    }
    assert_eq!(command_keys().len(), list.len(), "a key twice");
}

#[test]
fn the_form_maps_every_required_channel_and_only_channels_it_has() {
    let doc = read("forms.json");
    let form = &doc.as_array().expect("list")[0];
    assert_eq!(form["name"], "fields-in-groups");
    assert_eq!(form["version"], "1.0");
    let table =
        forms::find_key("fields-in-groups@1.0.0").expect("fields-in-groups@1.0.0 in fs-interfaces");
    let map = form["map"].as_object().expect("map");
    for channel in table.required() {
        assert!(
            map.contains_key(&channel.name),
            "required channel {} unmapped",
            channel.name
        );
    }
    let fields = form["fields"].as_object().expect("fields");
    let keys = command_keys();
    for (channel, to) in map {
        let ch = table
            .channel(channel)
            .unwrap_or_else(|| panic!("no channel {channel}"));
        let to = to.as_str().expect("one name");
        if ch.arts == [forms::Art::Command] {
            assert!(
                keys.contains(to),
                "{channel}: {to} is no command of commands.json"
            );
        } else if ch.arts != [forms::Art::Records] {
            assert!(
                fields.contains_key(to),
                "{channel}: field {to} not declared"
            );
        }
    }
    assert_eq!(
        map.len(),
        table.channels.len(),
        "every channel of the form is mapped"
    );
}
