//! The settings remembered in app.ron as a user's hold them: written by an older version, without
//! the choices added since (AGENTS.md, Verifying against the state outside the code).

use hydrobem_gui::app::Settings;

#[test]
fn settings_of_an_older_version_take_the_defaults_of_the_newer_choices() {
    // as saved before View → Show Normals and View → Show Spans existed, with choices of its own
    let old =
        r#"{"recent":[],"output_layout":"Tabs","show_field_points":true,"smoothing":false,"show_free_surface":false}"#;
    let s: Settings = serde_json::from_str(old).expect("an older settings text loads");
    // the user's choices are kept
    assert!(s.show_field_points && !s.smoothing && !s.show_free_surface);
    // the newer ones take their documented defaults: normals off, spans on
    assert!(!s.show_normals);
    assert!(s.show_spans);
    // and a text of the present version keeps what was chosen
    let now = Settings { show_normals: true, show_spans: false, ..Settings::default() };
    let back: Settings = serde_json::from_str(&serde_json::to_string(&now).unwrap()).unwrap();
    assert!(back.show_normals && !back.show_spans);
}
