Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(10)
    ResetEventObjective(15)
    ResetEventObjective(20)
    ResetEventObjective(90)
    ResetEventObjective(95)
    ResetEventObjective(100)
    ResetEventObjective(105)
    ResetEventObjective(110)
    ResetEventObjective(115)
    ResetEventObjective(120)
    ResetEventObjective(130)
    ResetEventObjective(140)
    ResetEventObjective(150)
    ResetEventObjective(160)
    ResetEventObjective(170)
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
    FailOpenObjective(15)
    FailOpenObjective(20)
    FailOpenObjective(90)
    FailOpenObjective(95)
    FailOpenObjective(100)
    FailOpenObjective(105)
    FailOpenObjective(110)
    FailOpenObjective(115)
    FailOpenObjective(120)
    FailOpenObjective(130)
    FailOpenObjective(140)
    FailOpenObjective(150)
    FailOpenObjective(160)
    FailOpenObjective(170)
EndFunction

Function StartAnnouncement(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(1500) || IsStageDone(3000)
EndFunction

Bool Function AreWavesClosed()
    Return IsEventResolved() || IsStageDone(1000) || IsStageDone(2000)
EndFunction

DefaultQuestEncounterWaveScript Function EventWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Bool Function IsStillDestroyed(Int aiStill)
    Return IsStageDone(790 + aiStill * 10)
EndFunction

Function StartWavePhase(Int aiWave, Int aiObjective)
    If AreWavesClosed()
        Return
    EndIf
    SetObjectiveDisplayed(aiObjective, True, True)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves == None
        Return
    EndIf
    Int still = 1
    While still <= 3
        ; A destroyed still has nothing left to attack, so its attackers stay home.
        If !IsStillDestroyed(still)
            waves.StartEncounterWaveByID("Wave" + aiWave + "_Still" + still + "_Ghouls")
            waves.StartEncounterWaveByID("Wave" + aiWave + "_Still" + still + "_StillGulpers")
        EndIf
        still += 1
    EndWhile
    waves.StartEncounterWaveByID("Wave" + aiWave + "_PlayerGulpers")
    waves.StartEncounterWaveByID("Wave" + aiWave + "_Other")
EndFunction

Function StopWavePhase(Int aiWave)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves == None
        Return
    EndIf
    Int still = 1
    While still <= 3
        waves.StopEncounterWaveByID("Wave" + aiWave + "_Still" + still + "_Ghouls")
        waves.StopEncounterWaveByID("Wave" + aiWave + "_Still" + still + "_StillGulpers")
        still += 1
    EndWhile
    waves.StopEncounterWaveByID("Wave" + aiWave + "_PlayerGulpers")
    waves.StopEncounterWaveByID("Wave" + aiWave + "_Other")
EndFunction

Function StopStillWaves(Int aiStill)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves == None
        Return
    EndIf
    Int wave = 1
    While wave <= 4
        waves.StopEncounterWaveByID("Wave" + wave + "_Still" + aiStill + "_Ghouls")
        waves.StopEncounterWaveByID("Wave" + wave + "_Still" + aiStill + "_StillGulpers")
        wave += 1
    EndWhile
EndFunction

Function BeginIntermission(Int aiWave, Int aiWaveObjective, Int aiIntermissionObjective)
    CompleteOpenObjective(aiWaveObjective)
    StopWavePhase(aiWave)
    If !AreWavesClosed()
        SetObjectiveDisplayed(aiIntermissionObjective, True, True)
    EndIf
EndFunction

Function EndIntermission(Int aiIntermissionObjective, Int aiWave, Int aiWaveObjective)
    CompleteOpenObjective(aiIntermissionObjective)
    StartWavePhase(aiWave, aiWaveObjective)
EndFunction

Function HandleStillDestroyed(Int aiStill, Int aiObjective)
    FailOpenObjective(aiObjective)
    StopStillWaves(aiStill)
    If !IsEventResolved()
        StartAnnouncement(PA_StillDestroyed)
    EndIf
    Quest owner = Self as Quest
    DefaultCounterQuest stillCounter = owner as DefaultCounterQuest
    If stillCounter != None
        stillCounter.Increment()
    EndIf
    Bool allDestroyed = True
    Int still = 1
    While still <= 3
        If still != aiStill && !IsStillDestroyed(still)
            allDestroyed = False
        EndIf
        still += 1
    EndWhile
    If allDestroyed && !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Function WarnStillNearlyDestroyed(Int aiStill, Message akWarning)
    If akWarning != None && !IsStillDestroyed(aiStill) && !AreWavesClosed()
        akWarning.Show()
    EndIf
EndFunction

Function SetBonfireLit(Bool abLit)
    If JamboreeFire == None
        Return
    EndIf
    E08A_BonfireScript bonfire = JamboreeFire.GetReference() as E08A_BonfireScript
    If bonfire == None
        Return
    EndIf
    If abLit
        bonfire.GoToState("BonfireLit")
    Else
        bonfire.TurnOffBonfire()
    EndIf
EndFunction

Function RepairEventReference(ReferenceAlias akAlias)
    If akAlias == None
        Return
    EndIf
    ObjectReference target = akAlias.GetReference()
    If target != None
        target.ClearDestruction()
    EndIf
EndFunction

Function ResetEventWorld()
    SetBonfireLit(False)
    RepairEventReference(Distiller1)
    RepairEventReference(Distiller2)
    RepairEventReference(Distiller3)
    RepairEventReference(Truck)
    ; Ned's greeting topic only offers the start of the jamboree while this value is 0.
    ActorValue spokenToNed = Game.GetFormFromFile(0x0064D540, "SeventySix.esm") as ActorValue
    If spokenToNed != None && Ned != None && Ned.GetActorReference() != None
        Ned.GetActorReference().SetValue(spokenToNed, 0.0)
    EndIf
EndFunction

Function ScheduleShutdown()
    CancelTimer(62161)
    StartTimer(10.0, 62161)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 62161 && IsRunning() && IsEventResolved()
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    CancelTimer(62161)
    ResetEventObjectives()
    ResetEventWorld()
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    CompleteOpenObjective(10)
    If !IsStageDone(160) && !IsEventResolved()
        SetObjectiveDisplayed(15, True, True)
    EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
    CompleteOpenObjective(10)
    CompleteOpenObjective(15)
    If IsStageDone(170) || IsEventResolved()
        Return
    EndIf
    SetObjectiveDisplayed(20, True, True)
    StartAnnouncement(PA_WaitTimeOver)
EndFunction

Function Fragment_Stage_0170_Item_00()
    CompleteOpenObjective(10)
    CompleteOpenObjective(15)
    CompleteOpenObjective(20)
    If IsEventResolved()
        Return
    EndIf
    SetBonfireLit(True)
    ; The jamboree fire sits on the truck bed, so lighting it sets the truck off.
    If Truck != None && Truck.GetReference() != None && ExplosionCar != None
        Truck.GetReference().PlaceAtMe(ExplosionCar)
    EndIf
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If IsEventResolved()
        Return
    EndIf
    StartAnnouncement(PA_TruckExploded)
    SetObjectiveDisplayed(130, True, True)
    SetObjectiveDisplayed(140, True, True)
    SetObjectiveDisplayed(150, True, True)
    SetObjectiveDisplayed(160, True, True)
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartWavePhase(1, 90)
    If !AreWavesClosed()
        StartAnnouncement(PA_WavesInProgress)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    BeginIntermission(1, 90, 95)
EndFunction

Function Fragment_Stage_0400_Item_00()
    EndIntermission(95, 2, 100)
EndFunction

Function Fragment_Stage_0450_Item_00()
    BeginIntermission(2, 100, 105)
EndFunction

Function Fragment_Stage_0500_Item_00()
    EndIntermission(105, 3, 110)
EndFunction

Function Fragment_Stage_0550_Item_00()
    BeginIntermission(3, 110, 115)
EndFunction

Function Fragment_Stage_0600_Item_00()
    EndIntermission(115, 4, 120)
EndFunction

Function Fragment_Stage_0800_Item_00()
    HandleStillDestroyed(1, 130)
EndFunction

Function Fragment_Stage_0805_Item_00()
    WarnStillNearlyDestroyed(1, Wave1)
EndFunction

Function Fragment_Stage_0810_Item_00()
    HandleStillDestroyed(2, 140)
EndFunction

Function Fragment_Stage_0815_Item_00()
    WarnStillNearlyDestroyed(2, Wave2)
EndFunction

Function Fragment_Stage_0820_Item_00()
    HandleStillDestroyed(3, 150)
EndFunction

Function Fragment_Stage_0825_Item_00()
    WarnStillNearlyDestroyed(3, Wave3)
EndFunction

Function Fragment_Stage_0900_Item_00()
    If !IsEventResolved()
        StartAnnouncement(PA_VenomDeposited)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    If !IsEventResolved()
        StartAnnouncement(PA_VenomHalfway)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteOpenObjective(120)
    StopWavePhase(4)
    If IsEventResolved()
        Return
    EndIf
    ; The jamboree succeeds when the fourth wave ends with at least one still standing.
    If IsStageDone(2000)
        SetStage(3000)
    ElseIf !IsStageDone(1400)
        SetStage(1400)
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    CompleteOpenObjective(160)
    If IsEventResolved()
        Return
    EndIf
    StartAnnouncement(PA_RequiredVenomGoal)
    If !IsStageDone(1200)
        SetObjectiveDisplayed(170, True, True)
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    CompleteOpenObjective(160)
    CompleteOpenObjective(170)
    If !IsEventResolved()
        StartAnnouncement(PA_ExtraVenomGoal)
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    If !IsEventResolved()
        SetStage(1500)
    EndIf
EndFunction

Function Fragment_Stage_1500_Item_00()
    If IsStageDone(3000)
        Return
    EndIf
    CompleteOpenObjective(120)
    CompleteOpenObjective(130)
    CompleteOpenObjective(140)
    CompleteOpenObjective(150)
    ; Venom deliveries are bonus tiers; an unfinished tier closes as failed.
    FailOpenObjectives()
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    StartAnnouncement(PA_EventSuccess)
    ScheduleShutdown()
EndFunction

Function Fragment_Stage_2000_Item_00()
    If !IsEventResolved()
        SetStage(3000)
    EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
    If IsStageDone(1500)
        Return
    EndIf
    FailOpenObjectives()
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    StartAnnouncement(PA_EventFailure)
    ScheduleShutdown()
EndFunction

Function Fragment_Stage_10000_Item_00()
    CancelTimer(62161)
    ResetEventWorld()
    Stop()
EndFunction
