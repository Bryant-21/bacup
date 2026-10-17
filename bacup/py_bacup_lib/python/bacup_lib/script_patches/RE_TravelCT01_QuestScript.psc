; RE_TravelCT01 Free Roaming Gutsy: the Free States Mr. Gutsy (alias 20 FreeStatesGutsy, created initially disabled)
; appears and carries the "The Free States" note (RE_TravelCT01_Note) as loot.
Event OnQuestInit()
    ReferenceAlias gutsyAlias = GetAlias(20) as ReferenceAlias
    If gutsyAlias == None
        Return
    EndIf
    ObjectReference gutsy = gutsyAlias.GetReference()
    If gutsy == None
        Return
    EndIf
    If gutsy.IsDisabled()
        gutsy.Enable()
    EndIf
    If RE_TravelCT01_Note != None && gutsy.GetItemCount(RE_TravelCT01_Note) == 0
        gutsy.AddItem(RE_TravelCT01_Note, 1, True)
    EndIf
EndEvent
