//! The report HOPatch writes beside a geometry file (`NAME.report.json` next to `NAME.hbg`,
//! HOPatch `docs/reference/cli.md`, format `hopatch-report` version 1): per patch, by its name, the CAD face
//! it came from, its checks and the recommended potential discretization. HOPatch leaves the
//! discretization out of the geometry file (the engine cannot change one read from a file), so a
//! mesh read from it takes the report's rows; the checks are shown with the patches.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// The report of a geometry file, its patches in the order of the mesh's patches. Saved with the
/// project (the GUI state), as the mesh it describes is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PatchReport {
    pub file: PathBuf,
    /// The state of the conversion: ok, warn or fail.
    pub state: String,
    pub patches: Vec<ReportPatch>,
    /// The CAD file converted (absolute, as HOPatch wrote it) and the recipe used, item by item:
    /// what Reimport CAD starts from.
    pub cad_file: PathBuf,
    pub recipe: Vec<(String, Json)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReportPatch {
    /// The patch's name in the geometry file.
    pub name: String,
    /// The CAD face (one-based) and its type (Plane, Cylinder, ...).
    pub face: i64,
    pub face_type: String,
    /// ok, warn or fail: the worst of its checks.
    pub state: String,
    /// The checks that are not ok, as "gap warn".
    pub problems: Vec<String>,
    /// The recommended discretization (nu, nv, ku, kv, iquo, iqui).
    pub discretization: Option<[i64; 6]>,
}

/// Where HOPatch writes the report of a geometry file.
pub fn report_path(hbg: &Path) -> PathBuf {
    let stem = hbg.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    hbg.with_file_name(format!("{stem}.report.json"))
}

impl PatchReport {
    /// The report beside the geometry file `hbg`, matched to the mesh's patch names; None when
    /// there is none, it is older than the file, it is not a HOPatch report, or a patch of the
    /// mesh is not in it.
    pub fn beside(hbg: &Path, names: &[String]) -> Option<PatchReport> {
        let file = report_path(hbg);
        // a report left from an earlier conversion (HOPatch's protocol 2 writes none) is not this
        // file's: HOPatch wrote the two together
        let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        if let (Some(h), Some(r)) = (modified(hbg), modified(&file))
            && h.duration_since(r).is_ok_and(|d| d.as_secs() > 60)
        {
            return None;
        }
        let text = std::fs::read_to_string(&file).ok()?;
        PatchReport::parse(&text, names).map(|mut r| {
            r.file = file;
            r
        })
    }

    /// A report's text matched to the patch names of a mesh.
    pub fn parse(text: &str, names: &[String]) -> Option<PatchReport> {
        let j: Json = serde_json::from_str(text).ok()?;
        // a later version of the same format only adds members
        if j.get("format")?.as_str()? != "hopatch-report" || j.get("version")?.as_i64()? < 1 {
            return None;
        }
        let text_of = |p: &Json, k: &str| p.get(k).and_then(Json::as_str).unwrap_or_default().to_string();
        let listed = j.get("patches")?.as_array()?;
        let patches = names
            .iter()
            .map(|name| {
                let p = listed.iter().find(|p| p.get("name").and_then(Json::as_str) == Some(name.as_str()))?;
                let problems = p
                    .get("checks")
                    .and_then(Json::as_object)
                    .map(|c| {
                        c.iter()
                            .filter_map(|(k, v)| v.as_str().filter(|s| *s != "ok").map(|s| format!("{k} {s}")))
                            .collect()
                    })
                    .unwrap_or_default();
                let row: Option<Vec<i64>> = p
                    .get("discretization")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().filter_map(Json::as_i64).collect());
                Some(ReportPatch {
                    name: name.clone(),
                    face: p.get("face").and_then(Json::as_i64).unwrap_or(0),
                    face_type: text_of(p, "face_type"),
                    state: text_of(p, "state"),
                    problems,
                    discretization: row.and_then(|r| r.try_into().ok()),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let state = j.pointer("/summary/state").and_then(Json::as_str).unwrap_or_default().to_string();
        let cad_file = PathBuf::from(j.pointer("/source/file").and_then(Json::as_str).unwrap_or_default());
        let recipe = j
            .pointer("/source/recipe")
            .and_then(Json::as_object)
            .map(|r| r.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        Some(PatchReport { file: PathBuf::new(), state, patches, cad_file, recipe })
    }

    /// Whether it describes the patches named `names`, in that order.
    pub fn matches(&self, names: &[String]) -> bool {
        self.patches.len() == names.len() && self.patches.iter().zip(names).all(|(p, n)| &p.name == n)
    }

    /// How many patches are in each state: ok, warn and fail.
    pub fn counts(&self) -> [usize; 3] {
        let n = |s: &str| self.patches.iter().filter(|p| p.state == s).count();
        [n("ok"), n("warn"), n("fail")]
    }

    /// The recommended rows of every patch, when the report has one for each.
    pub fn discretization(&self) -> Option<Vec<[i32; 6]>> {
        self.patches.iter().map(|p| p.discretization.map(|r| r.map(|v| v as i32))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"{"format": "hopatch-report", "version": 1, "summary": {"state": "warn"},
        "source": {"file": "D:\\cad\\hull.step", "recipe": {"unit": "mm", "size": 4.0}},
        "patches": [
          {"name": "face_3.2", "face": 3, "face_type": "Cylinder", "state": "warn",
           "checks": {"fold": "ok", "corner_angle": "warn"}, "discretization": [2, 3, 3, 3, 3, 4]},
          {"name": "face_3.1", "face": 3, "face_type": "Cylinder", "state": "ok",
           "checks": {"fold": "ok"}, "discretization": [3, 1, 3, 3, 3, 4], "future": 1}]}"#;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_patches_are_matched_by_name() {
        let r = PatchReport::parse(REPORT, &names(&["face_3.1", "face_3.2"])).expect("report");
        assert_eq!(r.state, "warn");
        assert_eq!(r.patches[0].state, "ok");
        assert_eq!(r.patches[1].problems, vec!["corner_angle warn".to_string()]);
        assert_eq!(r.discretization(), Some(vec![[3, 1, 3, 3, 3, 4], [2, 3, 3, 3, 3, 4]]));
        assert_eq!(r.cad_file, Path::new(r"D:\cad\hull.step"));
        // by item name (the dialog matches the items by name)
        assert_eq!(r.recipe, vec![("size".to_string(), Json::from(4.0)), ("unit".to_string(), Json::from("mm"))]);
        // a patch the report does not have: not this mesh's report
        assert!(PatchReport::parse(REPORT, &names(&["face_3.1", "face_4"])).is_none());
        assert!(PatchReport::parse(&REPORT.replace("hopatch-report", "other"), &names(&["face_3.1"])).is_none());
    }

    #[test]
    fn the_report_is_named_after_the_geometry_file() {
        assert_eq!(report_path(Path::new("out/OC4_Main.hbg")), Path::new("out/OC4_Main.report.json"));
    }
}
