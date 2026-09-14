Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
        If playerRef != None
            Alias_Player.ForceRefTo(playerRef)
        EndIf
    EndIf
    If playerRef != None
        If pBoS01StartedAV != None
            playerRef.SetValue(pBoS01StartedAV, 1.0)
        EndIf
        If pBoS01_QuestTrackerValue != None
            playerRef.SetValue(pBoS01_QuestTrackerValue, 1.0)
        EndIf
    EndIf
    If !IsObjectiveCompleted(100)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS01_QuestTrackerValue != None
        playerRef.SetValue(pBoS01_QuestTrackerValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsObjectiveCompleted(100)
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0210_Item_00()
    If !IsObjectiveCompleted(210)
        SetObjectiveDisplayed(210, True)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    If !IsObjectiveCompleted(220)
        SetObjectiveDisplayed(220, True)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    If IsObjectiveDisplayed(220) && !IsObjectiveCompleted(220)
        SetObjectiveCompleted(220, True)
    EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
    If IsObjectiveDisplayed(210) && !IsObjectiveCompleted(210)
        SetObjectiveCompleted(210, True)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0325_Item_00()
    If !IsObjectiveCompleted(300)
        SetObjectiveDisplayed(300, True)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    If pAlleghenyAsylumPoweredUp != None
        pAlleghenyAsylumPoweredUp.SetValue(1.0)
    EndIf
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(500, True)
    CompleteAllObjectives()

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    Bool checkpointAdvanced = False
    If playerRef != None
        If pBoS01CompletedAV != None
            playerRef.SetValue(pBoS01CompletedAV, 1.0)
        EndIf
        If pBoS01EndNoteAV != None
            playerRef.SetValue(pBoS01EndNoteAV, 1.0)
        EndIf
        If pBoS01_QuestTrackerValue != None
            playerRef.SetValue(pBoS01_QuestTrackerValue, 0.0)
        EndIf
        If pBoS01_CheckpointValue != None && playerRef.GetValue(pBoS01_CheckpointValue) < 1.0
            playerRef.SetValue(pBoS01_CheckpointValue, 1.0)
            checkpointAdvanced = True
        EndIf
    EndIf
    If checkpointAdvanced && pCheckpointMessage != None
        pCheckpointMessage.Show()
    EndIf

    CompleteQuest()

    If BoS01_TryStartBoS02()
        SetStage(600)
    Else
        StartTimer(5.0, 500)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && pBoS01_QuestTrackerValue != None
        playerRef.SetValue(pBoS01_QuestTrackerValue, 0.0)
    EndIf
    Alias_Player.Clear()
    Stop()
EndFunction

Bool Function BoS01_TryStartBoS02()
    If pBoS02 != None && (pBoS02.IsRunning() || pBoS02.IsCompleted())
        Return True
    EndIf

    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None || pBoS02_QuestStartKeyword == None
        Return False
    EndIf

    Bool accepted = pBoS02_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Bool started = accepted || (pBoS02 != None && (pBoS02.IsRunning() || pBoS02.IsCompleted()))
    If started && pBoS02StartedAV != None
        playerRef.SetValue(pBoS02StartedAV, 1.0)
    EndIf
    Return started
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 500 || !GetStageDone(500) || GetStageDone(600)
        Return
    EndIf

    If BoS01_TryStartBoS02()
        SetStage(600)
    Else
        StartTimer(5.0, 500)
    EndIf
EndEvent
