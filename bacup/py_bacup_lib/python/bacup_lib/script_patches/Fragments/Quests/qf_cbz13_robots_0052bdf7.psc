DefaultQuestEncounterWaveScript Function EventWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(0)
    ResetEventObjective(100)
    ResetEventObjective(300)
    ResetEventObjective(400)
    ResetEventObjective(500)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjectives()
    FailOpenObjective(0)
    FailOpenObjective(100)
    FailOpenObjective(300)
    FailOpenObjective(400)
    FailOpenObjective(500)
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function SetTruckDoorOpen(Bool abOpen)
    If Alias_TruckDoor == None
        Return
    EndIf
    ObjectReference doorRef = Alias_TruckDoor.GetReference()
    If doorRef != None
        doorRef.SetOpen(abOpen)
    EndIf
EndFunction

Function StartWavePhase(String asWaveID)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves == None
        Return
    EndIf
    waves.StartEncounterWaveByID(asWaveID)
EndFunction

Function StopEventWaves(Bool abRemoveActors)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Bool Function IsEndlessWavePhase(DefaultQuestEncounterWaveScript akWaves, Int aiWaveIndex)
    ; The three "MinusBoss" rows carry WaveType 0 (endless until stopped); a single
    ; subwave is one full single-player wave, and stopping the row lets its
    ; StageToSetAtEnd advance the phase once the robots are destroyed.
    If aiWaveIndex < 0 || akWaves == None
        Return False
    EndIf
    If aiWaveIndex == akWaves.FindEncounterWaveIndex("FirstWaveMinusBoss")
        Return True
    EndIf
    If aiWaveIndex == akWaves.FindEncounterWaveIndex("SecondWaveMinusBoss")
        Return True
    EndIf
    Return aiWaveIndex == akWaves.FindEncounterWaveIndex("ThirdWaveMinusBoss")
EndFunction

Function WatchWaveSpawns(Bool abWatch)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves == None
        Return
    EndIf
    If abWatch
        RegisterForCustomEvent(waves, "FirstSubwaveSpawned")
    Else
        UnregisterForCustomEvent(waves, "FirstSubwaveSpawned")
    EndIf
EndFunction

Event DefaultQuestEncounterWaveScript.FirstSubwaveSpawned(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If akSender == None || akArgs == None || akArgs.Length < 1
        Return
    EndIf
    Int waveIndex = akArgs[0] as Int
    If IsEndlessWavePhase(akSender, waveIndex)
        akSender.StopEncounterWave(waveIndex, False)
    EndIf
EndEvent

Function PowerDownQuestGiver()
    If CBZ13_Robots_Scene_ShutDown != None && !CBZ13_Robots_Scene_ShutDown.IsPlaying()
        CBZ13_Robots_Scene_ShutDown.Start()
    EndIf
    ; The shutdown scene's phase used to advance the quest; keep the recall running
    ; even when the converted scene cannot play.
    StartTimer(8.0, 5299)
EndFunction

Function MakeFinalBossEpic()
    ; FO76 handed the final boss to SQ_EpicCreatures ("Epic Levels"); the single-player
    ; substitute is the Tales legendary rank actor value.
    If Alias_FinalBossRefCollection == None || !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return
    EndIf
    ActorValue rankValue = Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
    If rankValue == None
        Return
    EndIf
    Int index = 0
    While index < Alias_FinalBossRefCollection.GetCount()
        Actor bossActor = Alias_FinalBossRefCollection.GetAt(index) as Actor
        If bossActor != None && bossActor.GetValue(rankValue) < 3.0
            bossActor.SetValue(rankValue, 3.0)
        EndIf
        index += 1
    EndWhile
EndFunction

Function ArmEventExpiry()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StartQuestTimer()
        Return
    EndIf
    ; The source quest carries its 30-minute expiry in QTEL (EventTimeMedium), which has
    ; no FO4 field and no converter-attached quest timer, so run it locally instead.
    Float seconds = 1800.0
    GlobalVariable expiryLength = Game.GetFormFromFile(0x00379693, "SeventySix.esm") as GlobalVariable
    If expiryLength != None && expiryLength.GetValue() > 0.0
        seconds = expiryLength.GetValue()
    EndIf
    CancelTimer(5298)
    StartTimer(seconds, 5298)
EndFunction

Function ScheduleEventShutdown()
    CancelTimer(5298)
    CancelTimer(5299)
    StartTimer(10.0, 5297)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 5297
        Stop()
    ElseIf aiTimerID == 5298
        If !IsStageDone(9000) && !IsStageDone(8000)
            SetStage(8000)
        EndIf
    ElseIf aiTimerID == 5299
        If !IsStageDone(210) && !IsStageDone(300)
            SetStage(210)
        EndIf
    EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
    ResetEventObjectives()
    WatchWaveSpawns(True)
    SetTruckDoorOpen(True)
    SetObjectiveDisplayed(0, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteOpenObjective(0)
    SetTruckDoorOpen(True)
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(0)
    CompleteOpenObjective(100)
    ArmEventExpiry()
    PowerDownQuestGiver()
EndFunction

Function Fragment_Stage_0210_Item_00()
    CancelTimer(5299)
    If Alias_QuestGiver != None && Alias_QuestGiver.GetReference() != None
        ; Sergeant Gutsy powers down to conserve energy once the recall is running.
        Alias_QuestGiver.GetReference().BlockActivation(True, True)
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(300, True, True)
    StartWavePhase("FirstWaveMinusBoss")
EndFunction

Function Fragment_Stage_0350_Item_00()
    StartWavePhase("FirstWaveBoss")
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteOpenObjective(300)
    SetObjectiveDisplayed(400, True, True)
    StartWavePhase("SecondWaveMinusBoss")
EndFunction

Function Fragment_Stage_0450_Item_00()
    StartWavePhase("SecondWaveBoss")
EndFunction

Function Fragment_Stage_0500_Item_00()
    CompleteOpenObjective(400)
    SetObjectiveDisplayed(500, True, True)
    StartWavePhase("ThirdWaveMinusBoss")
EndFunction

Function Fragment_Stage_0550_Item_00()
    StartWavePhase("ThirdWaveBoss")
EndFunction

Function Fragment_Stage_0555_Item_00()
    MakeFinalBossEpic()
EndFunction

Function Fragment_Stage_8000_Item_00()
    StopEventWaves(False)
    FailOpenObjectives()
    ScheduleEventShutdown()
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteOpenObjective(300)
    CompleteOpenObjective(400)
    CompleteOpenObjective(500)
    StopEventWaves(False)
    ScheduleEventShutdown()
EndFunction

Function Fragment_Stage_9999_Item_00()
    CancelTimer(5297)
    CancelTimer(5298)
    CancelTimer(5299)
    WatchWaveSpawns(False)
    StopEventWaves(True)
    SetTruckDoorOpen(False)
    If Alias_QuestGiver != None && Alias_QuestGiver.GetReference() != None
        Alias_QuestGiver.GetReference().BlockActivation(False, False)
    EndIf
EndFunction
