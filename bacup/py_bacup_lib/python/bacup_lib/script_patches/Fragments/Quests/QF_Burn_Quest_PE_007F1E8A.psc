Bool Function IsEventOver()
    Return IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9993) || IsStageDone(9994) || IsStageDone(9996) || IsStageDone(9997) || IsStageDone(9998) || IsStageDone(9999)
EndFunction

Bool Function OtherEndStageDone(Int aiStage)
    Int[] endStages = New Int[8]
    endStages[0] = 9000
    endStages[1] = 9991
    endStages[2] = 9993
    endStages[3] = 9994
    endStages[4] = 9996
    endStages[5] = 9997
    endStages[6] = 9998
    endStages[7] = 9999
    Int index = 0
    While index < endStages.Length
        If endStages[index] != aiStage && IsStageDone(endStages[index])
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Burn_PublicEvent_HelperScript Function EventHelper()
    Quest owner = Self as Quest
    Return owner as Burn_PublicEvent_HelperScript
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StartFeralMongWaves()
    StartEventWave("Burn_PE_FeralMongWave1")
    StartEventWave("Burn_PE_FeralMongWave2")
    StartEventWave("Burn_PE_FeralMongWave3")
    StartEventWave("Burn_PE_FeralMongWave4")
EndFunction

Function StopFeralMongWaves()
    StopEventWave("Burn_PE_FeralMongWave1")
    StopEventWave("Burn_PE_FeralMongWave2")
    StopEventWave("Burn_PE_FeralMongWave3")
    StopEventWave("Burn_PE_FeralMongWave4")
EndFunction

Function StartRustKingArmyWaves()
    StartEventWave("Burn_PE_RKMeleeWave")
    StartEventWave("Burn_PE_RKHeavyMeleeWave")
    StartEventWave("Burn_PE_RKRangedWave")
    StartEventWave("Burn_PE_RKHeavyRangedWave")
EndFunction

Function StopRustKingArmyWaves()
    StopEventWave("Burn_PE_RKMeleeWave")
    StopEventWave("Burn_PE_RKHeavyMeleeWave")
    StopEventWave("Burn_PE_RKRangedWave")
    StopEventWave("Burn_PE_RKHeavyRangedWave")
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(10)
    ResetEventObjective(20)
    ResetEventObjective(25)
    ResetEventObjective(30)
    ResetEventObjective(32)
    ResetEventObjective(35)
    ResetEventObjective(40)
    ResetEventObjective(45)
    ResetEventObjective(57)
    ResetEventObjective(58)
    ResetEventObjective(59)
    ResetEventObjective(60)
    ResetEventObjective(64)
    ResetEventObjective(65)
    ResetEventObjective(70)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjectives()
    FailOpenObjective(10)
    FailOpenObjective(20)
    FailOpenObjective(25)
    FailOpenObjective(30)
    FailOpenObjective(35)
    FailOpenObjective(40)
    FailOpenObjective(45)
    FailOpenObjective(57)
    FailOpenObjective(58)
    FailOpenObjective(59)
    FailOpenObjective(60)
    FailOpenObjective(64)
    FailOpenObjective(65)
    FailOpenObjective(70)
EndFunction

Function HideOptionalEmoteObjective()
    If IsObjectiveDisplayed(32) && !IsObjectiveCompleted(32)
        SetObjectiveDisplayed(32, False)
    EndIf
EndFunction

Function ShowEventMessage(Message akMessage)
    If akMessage != None
        akMessage.Show()
    EndIf
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function EnableAliasActor(ReferenceAlias akAlias)
    If akAlias == None
        Return
    EndIf
    Actor aliasActor = akAlias.GetActorReference()
    If aliasActor != None
        aliasActor.Enable()
        aliasActor.EvaluatePackage()
    EndIf
EndFunction

Function BeginTamerWait()
    If Alias_Tamer != None && Alias_Tamer.GetReference() != None
        RegisterForRemoteEvent(Alias_Tamer.GetReference(), "OnActivate")
    EndIf
EndFunction

Function StopTamerWait()
    If Alias_Tamer != None && Alias_Tamer.GetReference() != None
        UnregisterForRemoteEvent(Alias_Tamer.GetReference(), "OnActivate")
    EndIf
EndFunction

Function WatchConsort(ReferenceAlias akConsort)
    If akConsort != None && akConsort.GetActorReference() != None
        RegisterForRemoteEvent(akConsort.GetActorReference(), "OnDeath")
    EndIf
EndFunction

Function StopConsortWatch()
    If DeathclawConsort01 != None && DeathclawConsort01.GetActorReference() != None
        UnregisterForRemoteEvent(DeathclawConsort01.GetActorReference(), "OnDeath")
    EndIf
    If DeathclawConsort02 != None && DeathclawConsort02.GetActorReference() != None
        UnregisterForRemoteEvent(DeathclawConsort02.GetActorReference(), "OnDeath")
    EndIf
EndFunction

Bool Function IsConsortDown(ReferenceAlias akConsort)
    Return akConsort == None || akConsort.GetActorReference() == None || akConsort.GetActorReference().IsDead()
EndFunction

Function FinishEvent(Int aiEndStage, Bool abSucceeded, Scene akResultScene)
    If OtherEndStageDone(aiEndStage)
        Return
    EndIf
    StopTamerWait()
    StopConsortWatch()
    HideOptionalEmoteObjective()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
    If abSucceeded
        CompleteOpenObjective(45)
        CompleteOpenObjective(70)
    Else
        FailOpenObjectives()
    EndIf
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    Burn_PublicEvent_HelperScript helper = EventHelper()
    If helper != None
        helper.CloseEvent(akResultScene)
    Else
        Stop()
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    ; The Beastmaster intro choice fragment was not carried to FO4; talking to her accepts the event.
    If akActionRef != Game.GetPlayer() || !IsRunning() || !IsStageDone(100) || IsStageDone(200) || IsEventOver()
        Return
    EndIf
    SetStage(200)
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If !IsRunning() || !IsStageDone(720) || IsStageDone(750) || IsEventOver()
        Return
    EndIf
    If IsConsortDown(DeathclawConsort01) && IsConsortDown(DeathclawConsort02)
        SetStage(750)
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(10, True, True)
    BeginTamerWait()
    Burn_PublicEvent_HelperScript helper = EventHelper()
    If helper != None
        helper.BeginEvent()
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If !IsStageDone(200) && !IsEventOver()
        SetObjectiveDisplayed(10, True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    StopTamerWait()
    CompleteOpenObjective(10)
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(20, True, True)
    SetObjectiveDisplayed(25, True, True)
    StartFeralMongWaves()
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteOpenObjective(20)
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteOpenObjective(20)
    CompleteOpenObjective(25)
    StopFeralMongWaves()
    If IsEventOver()
        Return
    EndIf
    StartEventScene(Scene_TamerMove)
    ; TIF_Burn_E01_Gear_00843223 sets 420 when the scrap-done scene ends; this covers a scene that never plays.
    Burn_PublicEvent_HelperScript helper = EventHelper()
    If helper != None
        helper.StartStageTimer(iTimeToWait as Float, 420)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    If IsEventOver()
        Return
    EndIf
    ShowEventMessage(MessageArmorUp)
    SetObjectiveDisplayed(30, True, True)
    If !IsStageDone(470)
        SetObjectiveDisplayed(32, True, True)
        ShowEventMessage(MessageEmote)
    EndIf
    Burn_PublicEvent_HelperScript helper = EventHelper()
    If helper != None
        helper.StartArmorUpTimer(iTimeToWait as Float)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    CompleteOpenObjective(30)
    If IsEventOver()
        Return
    EndIf
    ; Objective 35 is the hidden Burn_E01_ArmoredDeathclawExitDelay timer that sets 500.
    SetObjectiveDisplayed(35, True)
EndFunction

Function Fragment_Stage_0470_Item_00()
    CompleteOpenObjective(32)
EndFunction

Function Fragment_Stage_0500_Item_00()
    CompleteOpenObjective(35)
    HideOptionalEmoteObjective()
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(40, True, True)
    Burn_PublicEvent_HelperScript helper = EventHelper()
    If helper != None
        helper.ReleaseDeathclaw()
    EndIf
    ; Burn_PE_TamedDeathclawMove sets 600 on completion.
    StartEventScene(DeathclawMoveScene)
    If helper != None
        ; Burn_E01_EscortTimer is 15 s; objective 40 lacks UsesTimer, so no converter timer row exists for it.
        helper.StartEscortTimer(15.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    CompleteOpenObjective(40)
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(45, True, True)
    SetObjectiveDisplayed(57, True, True)
    SetObjectiveDisplayed(58, True)
    StartRustKingArmyWaves()
EndFunction

Function Fragment_Stage_0690_Item_00()
    CompleteOpenObjective(58)
    StopRustKingArmyWaves()
EndFunction

Function Fragment_Stage_0695_Item_00()
    CompleteOpenObjective(57)
    CompleteOpenObjective(58)
    StopRustKingArmyWaves()
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(59, True, True)
EndFunction

Function Fragment_Stage_0697_Item_00()
    CompleteOpenObjective(59)
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(60, True, True)
    SetObjectiveDisplayed(64, True)
    StartEventWave("Burn_PE_SnallygasterWave")
EndFunction

Function Fragment_Stage_0699_Item_00()
    CompleteOpenObjective(64)
    StopEventWave("Burn_PE_SnallygasterWave")
EndFunction

Function Fragment_Stage_0700_Item_00()
    CompleteOpenObjective(60)
    CompleteOpenObjective(64)
    StopEventWave("Burn_PE_SnallygasterWave")
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(65, True, True)
    If !IsStageDone(710)
        SetStage(710)
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    If IsEventOver()
        Return
    EndIf
    ShowEventMessage(MessageMatriarch)
    StartEventScene(TamerMatriarchCommentScene)
EndFunction

Function Fragment_Stage_0720_Item_00()
    CompleteOpenObjective(65)
    If IsEventOver()
        Return
    EndIf
    EnableAliasActor(DeathclawMatriarch)
    EnableAliasActor(DeathclawConsort01)
    EnableAliasActor(DeathclawConsort02)
    WatchConsort(DeathclawConsort01)
    WatchConsort(DeathclawConsort02)
    StartEventScene(MatriarchMoveScene)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0750_Item_00()
    StopConsortWatch()
EndFunction

Function Fragment_Stage_0770_Item_00()
    CompleteOpenObjective(70)
    If !IsEventOver()
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    FinishEvent(9000, True, EventSuccessScene)
EndFunction

Function Fragment_Stage_9991_Item_00()
    FinishEvent(9991, False, None)
EndFunction

Function Fragment_Stage_9993_Item_00()
    FinishEvent(9993, False, None)
EndFunction

Function Fragment_Stage_9994_Item_00()
    FinishEvent(9994, False, ScrapFailScene)
EndFunction

Function Fragment_Stage_9996_Item_00()
    FinishEvent(9996, False, RustKingArmyFailScene)
EndFunction

Function Fragment_Stage_9997_Item_00()
    FinishEvent(9997, False, None)
EndFunction

Function Fragment_Stage_9998_Item_00()
    FinishEvent(9998, False, DeathclawMatriarchFailScene)
EndFunction

Function Fragment_Stage_9999_Item_00()
    FinishEvent(9999, False, TamedDeathclawFailScene)
EndFunction

Function Fragment_Stage_10000_Item_00()
    StopTamerWait()
    StopConsortWatch()
EndFunction
