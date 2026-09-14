Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(100)

    If CBZ03_Pre_MiscQuestObjective
        CBZ03_Pre_MiscQuestObjective.SetStage(100)
    EndIf

    If Alias_MapMarker && Alias_MapMarker.GetReference()
        Alias_MapMarker.GetReference().AddToMap()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Stop()
EndFunction
