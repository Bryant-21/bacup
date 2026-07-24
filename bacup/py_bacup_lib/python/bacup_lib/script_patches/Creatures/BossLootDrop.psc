; TODO
Event OnEffectStart(Actor akTarget, Actor akCaster)
    If DoOnce || LLE_Creature_Boss_Small_Dynamic == None
        Return
    EndIf
    DoOnce = True
    akTarget.AddItem(LLE_Creature_Boss_Small_Dynamic, 1)
EndEvent
