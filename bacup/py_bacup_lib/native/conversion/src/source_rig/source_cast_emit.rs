impl CapabilityTreeEmitter<'_> {
    fn emit_source_clip(&mut self, id: &str, node: &CapabilityGeneratorNode) {
        let CapabilityGeneratorNode::SourceClip {
            source,
            clip_name,
            user_data,
            bindings,
            triggers,
            mode,
            flags,
            crop_start_bits,
            crop_end_bits,
            start_time_bits,
            playback_speed_bits,
            enforced_duration_bits,
            user_controlled_time_fraction_bits,
        } = node
        else {
            unreachable!()
        };
        let clip = self
            .manifest
            .clips
            .iter()
            .find(|clip| clip.name == *clip_name)
            .expect("validated source clip declaration");
        let animation = escape_xml(internal_path(self.manifest, &clip.path));
        let binding_ref = self.emit_bindings(bindings);
        let triggers_ref = if triggers.is_empty() {
            "null".to_string()
        } else {
            let triggers_ref = self.allocate_id();
            let rows = triggers.iter().map(|trigger| format!("<hkobject><hkparam name=\"localTime\">{}</hkparam><hkparam name=\"event\">{}</hkparam><hkparam name=\"relativeToEndOfClip\">{}</hkparam><hkparam name=\"acyclic\">{}</hkparam><hkparam name=\"isAnnotation\">{}</hkparam></hkobject>", f32::from_bits(trigger.local_time_bits), self.emit_inline_event_property(&trigger.event), trigger.relative_to_end_of_clip, trigger.acyclic, trigger.is_annotation)).collect::<String>();
            self.output.push_str(&format!("    <hkobject name=\"{triggers_ref}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"{}\">{rows}</hkparam></hkobject>\n", triggers.len()));
            triggers_ref
        };
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbClipGenerator\" signature=\"0xd4cc9f6\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{}</hkparam><hkparam name=\"animationBundleName\"/><hkparam name=\"animationName\">{animation}</hkparam><hkparam name=\"triggers\">{triggers_ref}</hkparam><hkparam name=\"userPartitionMask\">0</hkparam><hkparam name=\"cropStartAmountLocalTime\">{}</hkparam><hkparam name=\"cropEndAmountLocalTime\">{}</hkparam><hkparam name=\"startTime\">{}</hkparam><hkparam name=\"playbackSpeed\">{}</hkparam><hkparam name=\"enforcedDuration\">{}</hkparam><hkparam name=\"userControlledTimeFraction\">{}</hkparam><hkparam name=\"animationBindingIndex\">-1</hkparam><hkparam name=\"mode\">{mode}</hkparam><hkparam name=\"flags\">{flags}</hkparam></hkobject>\n",
            escape_xml(&source.name), f32::from_bits(*crop_start_bits), f32::from_bits(*crop_end_bits),
            f32::from_bits(*start_time_bits), f32::from_bits(*playback_speed_bits),
            f32::from_bits(*enforced_duration_bits), f32::from_bits(*user_controlled_time_fraction_bits),
        ));
    }

    fn emit_source_cyclic_blend(&mut self, id: &str, node: &CapabilityGeneratorNode) {
        let CapabilityGeneratorNode::CyclicBlendTransition {
            source,
            bindings,
            user_data,
            generator,
            event_to_freeze_blend_value,
            event_to_cross_blend,
            transition_out_event,
            transition_in_event,
            f_blend_parameter_bits,
            f_transition_duration_bits,
            e_blend_curve,
        } = node
        else {
            unreachable!()
        };
        let child = self.emit_node(generator, None);
        let binding_ref = self.emit_bindings(bindings);
        let mut registry = havok_native::hkx::descriptors::DescriptorRegistry::for_contents_version(
            "hk_2014.1.0-r1",
        );
        let signature = registry
            .get("BSCyclicBlendTransitionGenerator")
            .expect("target descriptor")
            .expect("target cyclic descriptor")
            .signature
            .clone();
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"BSCyclicBlendTransitionGenerator\" signature=\"{signature}\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{}</hkparam><hkparam name=\"pBlenderGenerator\">{child}</hkparam><hkparam name=\"EventToFreezeBlendValue\">{}</hkparam><hkparam name=\"EventToCrossBlend\">{}</hkparam><hkparam name=\"TransitionOutEvent\">{}</hkparam><hkparam name=\"TransitionInEvent\">{}</hkparam><hkparam name=\"fBlendParameter\">{}</hkparam><hkparam name=\"fTransitionDuration\">{}</hkparam><hkparam name=\"eBlendCurve\">{e_blend_curve}</hkparam></hkobject>\n",
            escape_xml(&source.name), self.emit_inline_event_property(event_to_freeze_blend_value),
            self.emit_inline_event_property(event_to_cross_blend), self.emit_inline_event_property(transition_out_event),
            self.emit_inline_event_property(transition_in_event), f32::from_bits(*f_blend_parameter_bits),
            f32::from_bits(*f_transition_duration_bits),
        ));
    }

    fn emit_source_bone_weights(
        &mut self,
        weights: &super::manifest::CapabilityGeneratorBoneWeights,
    ) -> String {
        let id = self.allocate_id();
        let bindings = self.emit_bindings(&weights.bindings);
        let values = weights
            .weights_bits
            .iter()
            .map(|bits| f32::from_bits(*bits).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        self.output.push_str(&format!("    <hkobject name=\"{id}\" class=\"hkbBoneWeightArray\" signature=\"0xcd902b77\"><hkparam name=\"variableBindingSet\">{bindings}</hkparam><hkparam name=\"boneWeights\" numelements=\"{}\">{values}</hkparam></hkobject>\n", weights.weights_bits.len()));
        id
    }

    fn emit_source_expression_condition(
        &mut self,
        condition: &super::manifest::CapabilityGeneratorExpressionCondition,
    ) -> String {
        let id = self.allocate_id();
        self.output.push_str(&format!("    <hkobject name=\"{id}\" class=\"hkbExpressionCondition\" signature=\"0x78a69526\"><hkparam name=\"expression\">{}</hkparam></hkobject>\n", escape_xml(&condition.expression)));
        id
    }
}
