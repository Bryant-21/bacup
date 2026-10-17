; Item aliases are optional: only the objectives whose item was picked for this request are shown.
Function DisplayObjectiveIfAliasFilled(Int aiAliasID, Int aiObjective)
    ReferenceAlias itemAlias = GetAlias(aiAliasID) as ReferenceAlias
    If itemAlias != None && itemAlias.GetReference() != None
        SetObjectiveDisplayed(aiObjective, True)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    DisplayObjectiveIfAliasFilled(1, 10)
    DisplayObjectiveIfAliasFilled(12, 11)
    DisplayObjectiveIfAliasFilled(16, 12)
    DisplayObjectiveIfAliasFilled(2, 15)
    DisplayObjectiveIfAliasFilled(15, 16)
    DisplayObjectiveIfAliasFilled(17, 17)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(10, True)
EndFunction

Function Fragment_Stage_0011_Item_00()
    SetObjectiveCompleted(11, True)
EndFunction

Function Fragment_Stage_0012_Item_00()
    SetObjectiveCompleted(12, True)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveCompleted(15, True)
EndFunction

Function Fragment_Stage_0016_Item_00()
    SetObjectiveCompleted(16, True)
EndFunction

Function Fragment_Stage_0017_Item_00()
    SetObjectiveCompleted(17, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(60, True)
EndFunction
