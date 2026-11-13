//! Geometry files (`.hbg`) of version 2, as cad2bem still writes them, read as version 3: the
//! engine reads version 3 only (HydroBEM 0.3.0). Version 3 renames the mesh symmetry by its
//! planes ("x" the plane x = 0 is "yz", "y" is "xz", "xy" is "xz_and_yz"); nothing else a file
//! holds changed. A file of any other version is left to the engine, which says why it refuses it.

use serde_json::Value;

/// The version the engine reads.
pub const HBG_VERSION: i64 = 3;

/// The text of a version-3 geometry file for the text of a version-2 one; None for any other
/// text (another version, or no geometry file at all).
pub fn upgrade(text: &str) -> Option<String> {
    let mut v: Value = serde_json::from_str(text).ok()?;
    let top = v.as_object_mut()?;
    if top.get("format").and_then(Value::as_str) != Some("hbg") || top.get("version").and_then(Value::as_i64) != Some(2)
    {
        return None;
    }
    top.insert("version".into(), Value::from(HBG_VERSION));
    if let Some(Value::String(symmetry)) = top.get_mut("symmetry") {
        let renamed = match symmetry.as_str() {
            "x" => "yz",
            "y" => "xz",
            "xy" => "xz_and_yz",
            other => other,
        };
        *symmetry = renamed.to_string();
    }
    serde_json::to_string(&v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_2_reads_as_version_3_with_the_symmetry_by_its_planes() {
        let v2 =
            r#"{"format": "hbg", "version": 2, "kind": "panels", "symmetry": "xy", "vertices": [[0.5, 1.0, 2.0]]}"#;
        let v3: Value = serde_json::from_str(&upgrade(v2).expect("upgraded")).unwrap();
        assert_eq!(v3["version"], 3);
        assert_eq!(v3["symmetry"], "xz_and_yz");
        assert_eq!(v3["vertices"][0][0], 0.5);
        for (old, new) in [("x", "yz"), ("y", "xz"), ("none", "none")] {
            let text = format!(r#"{{"format": "hbg", "version": 2, "symmetry": "{old}"}}"#);
            let v3: Value = serde_json::from_str(&upgrade(&text).unwrap()).unwrap();
            assert_eq!(v3["symmetry"], new);
        }
    }

    #[test]
    fn other_versions_and_other_texts_are_left_alone() {
        assert!(upgrade(r#"{"format": "hbg", "version": 3, "symmetry": "yz"}"#).is_none());
        assert!(upgrade(r#"{"format": "hbg", "version": 1}"#).is_none());
        assert!(upgrade(r#"{"format": "other", "version": 2}"#).is_none());
        assert!(upgrade("not json").is_none());
    }
}
