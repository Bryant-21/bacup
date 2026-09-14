; TODO

Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_PlayerAlias.GetActorReference()
    If playerRef
        playerRef.SetValue(Tutorial_PlaceCAMPStarted, 1.0)
    EndIf
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
EndFunction
