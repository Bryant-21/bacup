; Stage 2 is the retained debug flag: the source conditions on stages 60/63/64
; skip the hostile waves once it is set. Nothing else to restore.
Function Fragment_Stage_0002_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_BeginStartup()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    If ENz04_Bots_0005_IntroAttract != None && ENz04_Bots_0005_IntroAttract.IsPlaying()
        ENz04_Bots_0005_IntroAttract.Stop()
    EndIf
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_BeginConstruction()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(50)
    If ENz04_Bots_0050_DefendBots != None
        ENz04_Bots_0050_DefendBots.Start()
    EndIf
    ENz04_SetMarkerEnabled(Alias_PowerUpMarker, True)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_BeginDefense()
    EndIf
EndFunction

Function Fragment_Stage_0055_Item_00()
    ENz04_SetMarkerEnabled(Alias_MusicMarker, True)
EndFunction

Function Fragment_Stage_0060_Item_00()
    Bool announced = False
    If ENz04_Bots_0060_IncomingHostiles != None
        ENz04_Bots_0060_IncomingHostiles.Start()
        announced = ENz04_Bots_0060_IncomingHostiles.IsPlaying()
    EndIf
    If !announced
        ENz04_SayFromTerminal(ENz04_HostilesDetected)
    EndIf
    ENz04_StartWave(0)
EndFunction

Function Fragment_Stage_0063_Item_00()
    ENz04_SayFromTerminal(ENz04_MoreHostilesDetected)
    ENz04_StartWave(1)
EndFunction

Function Fragment_Stage_0064_Item_00()
    ENz04_SayFromTerminal(ENz04_MoreHostilesDetected)
    ENz04_StartWave(2)
EndFunction

; Wave three has ended; the boss wave waits until the bots are online.
Function Fragment_Stage_0065_Item_00()
    If IsStageDone(68)
        ENz04_StartBossWave()
    EndIf
EndFunction

Function Fragment_Stage_0068_Item_00()
    SetObjectiveCompleted(50)
    ENz04_SetMarkerEnabled(Alias_PowerUpMarker, False)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_BotsOnline()
    EndIf
    If IsStageDone(2)
        SetStage(70)
    ElseIf IsStageDone(69)
        SetStage(70)
    ElseIf IsStageDone(65)
        ENz04_StartBossWave()
    EndIf
EndFunction

Function Fragment_Stage_0069_Item_00()
    If IsStageDone(68)
        SetStage(70)
    EndIf
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveDisplayed(70)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_StepOutPatrol()
    EndIf
EndFunction

; No source producer for stage 71 was found.
Function Fragment_Stage_0071_Item_00()
EndFunction

; Stage 72 is set only by INFO 12B7B1 under the unreferenced topic 10F081.
Function Fragment_Stage_0072_Item_00()
EndFunction

Function Fragment_Stage_0175_Item_00()
    ENz04_SayFromTerminal(ENz04_TimerExpired)
    SetStage(180)
EndFunction

Function Fragment_Stage_0180_Item_00()
    If IsObjectiveDisplayed(50) && !IsObjectiveCompleted(50)
        SetObjectiveFailed(50)
    EndIf
    If IsObjectiveDisplayed(70) && !IsObjectiveCompleted(70)
        SetObjectiveFailed(70)
    EndIf
    ENz04_StopScenes()
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0190_Item_00()
    SetObjectiveCompleted(70)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_AnnouncePatrol()
        If controller.PatrolCenterMarker != None
            ENz04_PatrolHandlerScript patrolHandler = controller.ENz04_PatrolHandler as ENz04_PatrolHandlerScript
            If patrolHandler != None
                patrolHandler.TransferPatrol(controller.PatrolCenterMarker.GetReference(), Alias_PatrolBots)
            EndIf
        EndIf
    EndIf
    EnclaveEventQuestScript eventQuest = (Self as Quest) as EnclaveEventQuestScript
    If eventQuest != None
        eventQuest.ENEvent_RecordCompletion()
    EndIf
    If controller != None
        controller.ENz04_BeginWrapUp()
    EndIf
EndFunction

Function Fragment_Stage_0199_Item_00()
    ENz04_StopScenes()
    ENz04_SetMarkerEnabled(Alias_MusicMarker, False)
    ENz04_SetMarkerEnabled(Alias_PowerUpMarker, False)
EndFunction

Function Fragment_Stage_0200_Item_00()
    Stop()
EndFunction

Function ENz04_StartWave(Int aiWaveIndex)
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_StartHostileWave(aiWaveIndex)
    EndIf
EndFunction

Function ENz04_StartBossWave()
    ENz04_BotScript controller = (Self as Quest) as ENz04_BotScript
    If controller != None
        controller.ENz04_StartHostileWave(controller.iBossWaveIndex)
    EndIf
EndFunction

Function ENz04_SayFromTerminal(Topic akTopic)
    If akTopic == None || Alias_SpeakingTerminal == None
        Return
    EndIf
    ObjectReference speaker = Alias_SpeakingTerminal.GetReference()
    If speaker != None
        speaker.Say(akTopic)
    EndIf
EndFunction

Function ENz04_SetMarkerEnabled(ReferenceAlias akMarkerAlias, Bool abEnabled)
    If akMarkerAlias == None
        Return
    EndIf
    ObjectReference marker = akMarkerAlias.GetReference()
    If marker == None
        Return
    EndIf
    If abEnabled
        marker.EnableNoWait()
    Else
        marker.DisableNoWait()
    EndIf
EndFunction

Function ENz04_StopScenes()
    If ENz04_Bots_0005_IntroAttract != None && ENz04_Bots_0005_IntroAttract.IsPlaying()
        ENz04_Bots_0005_IntroAttract.Stop()
    EndIf
    If ENz04_Bots_0050_DefendBots != None && ENz04_Bots_0050_DefendBots.IsPlaying()
        ENz04_Bots_0050_DefendBots.Stop()
    EndIf
    If ENz04_Bots_0060_IncomingHostiles != None && ENz04_Bots_0060_IncomingHostiles.IsPlaying()
        ENz04_Bots_0060_IncomingHostiles.Stop()
    EndIf
EndFunction
