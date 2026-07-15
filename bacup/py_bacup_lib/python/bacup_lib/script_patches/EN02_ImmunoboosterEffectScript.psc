Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != None && EN02_PlayerImmunizedValue != None
        akTarget.SetValue(EN02_PlayerImmunizedValue, 1.0)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget != None && EN02_PlayerImmunizedValue != None
        akTarget.SetValue(EN02_PlayerImmunizedValue, 0.0)
    EndIf
EndEvent
