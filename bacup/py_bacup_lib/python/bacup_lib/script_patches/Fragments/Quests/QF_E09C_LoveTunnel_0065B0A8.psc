Function ResetEventObjectives()
    Int[] objectives = New Int[9]
    objectives[0] = 10
    objectives[1] = 20
    objectives[2] = 30
    objectives[3] = 35
    objectives[4] = 40
    objectives[5] = 45
    objectives[6] = 60
    objectives[7] = 65
    objectives[8] = 70
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function FailOpenObjectives()
    Int[] objectives = New Int[9]
    objectives[0] = 10
    objectives[1] = 20
    objectives[2] = 30
    objectives[3] = 35
    objectives[4] = 40
    objectives[5] = 45
    objectives[6] = 60
    objectives[7] = 65
    objectives[8] = 70
    Int index = 0
    While index < objectives.Length
        Int objective = objectives[index]
        If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
            SetObjectiveFailed(objective, True)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetWeddingNetState(String asState)
    If Alias_WeddingNet == None
        Return
    EndIf
    E09C_WeddingNetScript weddingNet = Alias_WeddingNet.GetReference() as E09C_WeddingNetScript
    If weddingNet != None && weddingNet.GetState() != asState
        weddingNet.GoToState(asState)
    EndIf
EndFunction

Function ScheduleShutdown()
    ; Stage rewards (B21:QuestRewards, Nuka-Cade points) are granted from the same stage's OnStageSet.
    StartTimer(5.0, 65908)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 65908 && IsRunning()
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetWeddingNetState("initial")
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0120_Item_00()
    SetObjectiveCompleted(20)
    If Alias_TunnelIntroDoor
        ObjectReference introDoor = Alias_TunnelIntroDoor.GetReference()
        If introDoor
            introDoor.Lock(False)
            introDoor.SetOpen(True)
        EndIf
    EndIf
    SetObjectiveDisplayed(30)
    If E09C_LoveTunnel_PA_StartDecorations && !E09C_LoveTunnel_PA_StartDecorations.IsPlaying()
        E09C_LoveTunnel_PA_StartDecorations.Start()
    EndIf
EndFunction

Function Fragment_Stage_0190_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0290_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(45)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(45)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0350_Item_00()
    If Alias_MrHandy && E09C_MrLovelyMoveToWeddingMarker
        ObjectReference mrLovely = Alias_MrHandy.GetReference()
        If mrLovely
            mrLovely.MoveTo(E09C_MrLovelyMoveToWeddingMarker)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0390_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(65)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(65)
    SetObjectiveDisplayed(70)
    If E09C_WeddingDialogueEnabled
        E09C_WeddingDialogueEnabled.SetValue(1.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(70)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsObjectiveDisplayed(70) && !IsObjectiveCompleted(70)
        SetObjectiveCompleted(70)
    EndIf
    ; The balloon net over the wedding drops its balloons for the finished ceremony.
    SetWeddingNetState("breaknet")
    ScheduleShutdown()
EndFunction

Function Fragment_Stage_9900_Item_00()
    ; DefaultSetStageOnQuestTimerEnd drives this stage when the event clock runs out.
    ; A run that already reached the wedding payoff must not be converted into a failure.
    If !GetStageDone(500) && !GetStageDone(9000)
        SetStage(9990)
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailOpenObjectives()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    ScheduleShutdown()
EndFunction

Function Fragment_Stage_10000_Item_00()
    CancelTimer(65908)
    If E09C_WeddingDialogueEnabled
        E09C_WeddingDialogueEnabled.SetValue(0.0)
    EndIf
    SetWeddingNetState("initial")
EndFunction
