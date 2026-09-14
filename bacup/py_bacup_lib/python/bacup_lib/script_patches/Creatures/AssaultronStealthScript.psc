Event OnEffectStart(Actor akTarget, Actor akCaster)
	If akTarget == None || StealthSpell == None
		Return
	EndIf
	Bool mobile = (LeftMobilityCondition == None || akTarget.GetValue(LeftMobilityCondition) > 0.0) || (RightMobilityCondition == None || akTarget.GetValue(RightMobilityCondition) > 0.0)
	Bool armed = (LeftAttackCondition == None || akTarget.GetValue(LeftAttackCondition) > 0.0) || (RightAttackCondition == None || akTarget.GetValue(RightAttackCondition) > 0.0)
	Bool powered = EnduranceCondition == None || akTarget.GetValue(EnduranceCondition) > 0.0
	If mobile && armed && powered
		akTarget.AddSpell(StealthSpell, False)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	If akTarget != None && StealthSpell != None
		akTarget.RemoveSpell(StealthSpell)
	EndIf
EndEvent
