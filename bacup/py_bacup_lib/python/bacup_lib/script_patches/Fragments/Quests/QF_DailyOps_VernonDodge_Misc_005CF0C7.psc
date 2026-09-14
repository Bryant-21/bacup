Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(35, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && AV_ReadyToTurnIn != None
        playerRef.SetValue(AV_ReadyToTurnIn, 1.0)
    EndIf
    SetObjectiveCompleted(35, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && AV_ReadyToTurnIn != None
        playerRef.SetValue(AV_ReadyToTurnIn, 0.0)
    EndIf
    SetObjectiveCompleted(40, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteQuest()
    Stop()
EndFunction
