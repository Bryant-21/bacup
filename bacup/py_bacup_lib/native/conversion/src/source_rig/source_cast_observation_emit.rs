impl CapabilityTreeEmitter<'_> {
    fn emit_cast_state_enter(&mut self, state: &super::manifest::CapabilityGeneratorState) -> String {
        let observations = self.cast_observations.iter().filter(|observation| {
            observation.source_behavior_path == state.source.behavior_path
                && observation.source_state_object_index == state.source.object_index
        }).collect::<Vec<_>>();
        if observations.is_empty() {
            return self.emit_event_properties(&state.enter_notify_events);
        }
        let mut rows = String::new();
        for observation in &observations {
            let event_id = self.declarations.events.iter()
                .position(|event| event.name == observation.target_event_name)
                .expect("source cast observation event is declared");
            rows.push_str(&format!("<hkobject><hkparam name=\"id\">{event_id}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject>"));
        }
        rows.extend(state.enter_notify_events.iter().map(|event| self.emit_inline_event_property(event)));
        let count = state.enter_notify_events.len() + observations.len();
        let id = self.allocate_id();
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\"><hkparam name=\"events\" numelements=\"{}\">{rows}</hkparam></hkobject>\n",
            count
        ));
        id
    }
}

fn source_cast_outer_events(role: &CapabilityClipRole, declarations: &GraphDeclarations) -> (Vec<String>, Vec<String>) {
    let CapabilityRoleGenerator::SourceCasting { enter_events, exit_events, .. } = role.generator.base_generator() else {
        return (Vec::new(), Vec::new());
    };
    let rows = |events: &[CapabilityGeneratorEventProperty]| events.iter().map(|event| {
        let id = event.event.event_name.as_ref().map_or(-1, |name| declarations.events.iter()
            .position(|declaration| declaration.name == *name).expect("source cast outer event is declared") as i32);
        format!("<hkobject><hkparam name=\"id\">{id}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject>")
    }).collect();
    (rows(enter_events), rows(exit_events))
}
