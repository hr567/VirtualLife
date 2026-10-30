//! Cache resource predictions at semantic scene boundaries, never per display tick.
use super::*;

pub(super) fn update(c: &SceneContent<'_>, snapshot: &Value, ui: &mut PresentationState) {
    let pending = &snapshot["pending"];
    let stage = n(snapshot, "/life/stage");
    let stage_rule = a(c.rules, "/stages")
        .iter()
        .find(|value| n(value, "/id") == stage);
    let approaching = stage_rule.is_some_and(|rule| {
        n(rule, "/stageTurns") > 0 && n(rule, "/stageTurns") - n(snapshot, "/life/stageTurn") <= 5
    });
    let inventory: Vec<_> = a(pending, "/state/entries")
        .iter()
        .map(|entry| entry["id"].clone())
        .collect();
    let key = json!([
        format!("{:?}", ui.screen),
        snapshot["mapId"],
        snapshot["player"]["sex"],
        stage,
        snapshot["life"]["profession"],
        approaching,
        pending["kind"],
        pending["game"],
        pending["state"]["phase"],
        pending["state"]["tab"],
        pending["state"]["page"],
        pending["state"]["input"],
        pending["state"]["background"],
        pending["state"]["enemy"],
        pending["state"]["playerClass"],
        pending["state"]["victory"],
        inventory,
        snapshot["scene"],
        pending["housing"]
    ])
    .to_string();
    if key == ui.runtime.preload_key {
        return;
    }
    ui.runtime.preload_key = key;
    ui.runtime.preload_resources =
        crate::prefetch::plan(c.manifest, c.maps, c.rules, snapshot, ui.screen);
}
