Function Fragment_Stage_0100_Item_00()
    ShowBuildObjective(100)
EndFunction

Function Fragment_Stage_0101_Item_00()
    FinishBuildObjective(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    ShowBuildObjective(200)
EndFunction

Function Fragment_Stage_0201_Item_00()
    FinishBuildObjective(200)
EndFunction

Function Fragment_Stage_0300_Item_00()
    ShowBuildObjective(300)
EndFunction

Function Fragment_Stage_0301_Item_00()
    FinishBuildObjective(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    ShowBuildObjective(400)
EndFunction

Function Fragment_Stage_0401_Item_00()
    FinishBuildObjective(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    ShowBuildObjective(500)
EndFunction

Function Fragment_Stage_0501_Item_00()
    FinishBuildObjective(500)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction

Function ShowBuildObjective(Int objective)
    If !IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective)
        SetObjectiveDisplayed(objective)
    EndIf
EndFunction

Function FinishBuildObjective(Int objective)
    If !IsObjectiveCompleted(objective)
        SetObjectiveCompleted(objective)
    EndIf
    COMP_BuildCampObjectObjectivesScript objectives = (Self as Quest) as COMP_BuildCampObjectObjectivesScript
    If objectives
        objectives.CompleteIfAllBuilt()
    EndIf
EndFunction
