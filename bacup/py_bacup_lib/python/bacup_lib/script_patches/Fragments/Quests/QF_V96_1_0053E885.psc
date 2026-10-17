Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0021_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(21, True)
EndFunction

Function Fragment_Stage_0193_Item_00()
    SetObjectiveCompleted(21, True)
    SetObjectiveDisplayed(193, True)
EndFunction

Function Fragment_Stage_0291_Item_00()
    SetObjectiveCompleted(193, True)
    SetObjectiveDisplayed(291, True)
EndFunction

Function Fragment_Stage_0391_Item_00()
    SetObjectiveCompleted(291, True)
    SetObjectiveDisplayed(391, True)
EndFunction

Function Fragment_Stage_0395_Item_00()
    SetObjectiveCompleted(391, True)
    SetObjectiveDisplayed(395, True)
EndFunction

Function Fragment_Stage_0396_Item_00()
    SetObjectiveCompleted(395, True)
    SetObjectiveDisplayed(396, True)
EndFunction

Function Fragment_Stage_0491_Item_00()
    SetObjectiveCompleted(396, True)
    SetObjectiveDisplayed(491, True)
EndFunction

Function Fragment_Stage_0591_Item_00()
    SetObjectiveCompleted(491, True)
    SetObjectiveDisplayed(591, True)
EndFunction

Function Fragment_Stage_0601_Item_00()
    SetObjectiveCompleted(591, True)
    SetObjectiveDisplayed(601, True)
EndFunction

Function Fragment_Stage_0611_Item_00()
    SetObjectiveCompleted(601, True)
    SetObjectiveDisplayed(611, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(611, True)
    SetObjectiveDisplayed(1000, True)
EndFunction

Function Fragment_Stage_1010_Item_00()
    CompleteAllObjectives()
    ; No Stop(): the reward stages 1011-1013 follow success, and this RunOnce quest cannot restart to take them.
EndFunction

Function Fragment_Stage_1100_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
