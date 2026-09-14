Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && TW005status != None && playerRef.GetValue(TW005status) < 1.0
        playerRef.SetValue(TW005status, 1.0)
    EndIf
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    TW005MayorIntro.Start()
    SetObjectiveDisplayed(201, True)
    SetObjectiveDisplayed(202, True)
    SetObjectiveDisplayed(203, True)
    SetObjectiveDisplayed(204, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(201, True)
    If !IsStageDone(301)
        SetStage(301)
    EndIf
    If IsStageDone(400) && IsStageDone(500) && IsStageDone(600)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(202, True)
    If !IsStageDone(401)
        SetStage(401)
    EndIf
    If IsStageDone(300) && IsStageDone(500) && IsStageDone(600)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(203, True)
    If !IsStageDone(501)
        SetStage(501)
    EndIf
    If IsStageDone(300) && IsStageDone(400) && IsStageDone(600)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(204, True)
    If !IsStageDone(601)
        SetStage(601)
    EndIf
    If IsStageDone(300) && IsStageDone(400) && IsStageDone(500)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && TW005status != None && playerRef.GetValue(TW005status) < 2.0
        playerRef.SetValue(TW005status, 2.0)
    EndIf
    SetObjectiveCompleted(201, True)
    SetObjectiveCompleted(202, True)
    SetObjectiveCompleted(203, True)
    SetObjectiveCompleted(204, True)
    CompleteQuest()
    Stop()
EndFunction
