Event OnEffectStart(Actor akTarget, Actor akCaster)
    If FestiveLeveledList == None
        Return
    EndIf
    akTarget.AddItem(FestiveLeveledList, 1)
EndEvent
