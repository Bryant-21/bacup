Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(70, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(50, True)
    ; The FO76 decryption scene that sets 60 has no FO4 scene fragment, so advance here.
    If !IsStageDone(60)
        SetStage(60)
    EndIf
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(70, True)
    CompleteQuest()
EndFunction
