Event OnEffectStart(Actor akTarget, Actor akCaster)
    If akTarget != None && SkinScorchedStatue != None
        akTarget.EquipItem(SkinScorchedStatue, True, True)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    If akTarget != None && SkinScorchedStatue != None
        akTarget.RemoveItem(SkinScorchedStatue, 1, True)
    EndIf
EndEvent
