Bool Function IsPatrolOver()
    Return IsStageDone(230) || IsStageDone(999)
EndFunction

Bool Function IsRoute2()
    Return IsStageDone(12)
EndFunction

Function ResetPatrolObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetPatrolObjectives()
    ResetPatrolObjective(0)
    ResetPatrolObjective(10)
    ResetPatrolObjective(11)
    ResetPatrolObjective(30)
    ResetPatrolObjective(140)
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
    FailOpenObjective(0)
    FailOpenObjective(10)
    FailOpenObjective(11)
    FailOpenObjective(30)
    FailOpenObjective(140)
EndFunction

TW043QuestScript Function PatrolScript()
    Quest owner = Self as Quest
    Return owner as TW043QuestScript
EndFunction

Function SetStageOnTimer(Int aiStage, Float afSeconds)
    If aiStage <= 0 || IsStageDone(aiStage) || IsPatrolOver() || !IsRunning()
        Return
    EndIf
    ; FO76 advanced these stages from placed trigger boxes; the port keeps the order on a timer.
    StartTimer(afSeconds, 45000 + aiStage)
EndFunction

Function StartPatrolLeg(ObjectReference akPatrolStart, Int aiArrivalStage)
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.StartEscortPatrol(akPatrolStart, aiArrivalStage)
    EndIf
EndFunction

Function StartPatrolWave(Int aiWaveIndex)
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
    If waveScript != None
        ; Neither patrol wave carries an IDString, so they can only be started by index.
        waveScript.StartEncounterWave(aiWaveIndex)
    EndIf
EndFunction

Function StopPatrolWaves()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function TeleportGuard(ReferenceAlias akMarkerAlias)
    If akMarkerAlias == None || Alias_LC043Guard == None
        Return
    EndIf
    ObjectReference marker = akMarkerAlias.GetReference()
    Actor guardRef = Alias_LC043Guard.GetActorReference()
    If marker == None || guardRef == None
        Return
    EndIf
    guardRef.MoveTo(marker)
EndFunction

Int Function StationObjective(Bool abStationD)
    ; TW043QuestScript's CONSTs pair station A with objective 30 and station D with objective 140.
    If abStationD
        Return 140
    EndIf
    Return 30
EndFunction

Int Function StationCompleteStage(Bool abStationD)
    If abStationD
        Return 170
    EndIf
    Return 60
EndFunction

Function BeginStationDefense(Bool abStationD)
    TW043QuestScript patrol = PatrolScript()
    If patrol == None || IsPatrolOver()
        Return
    EndIf

    Int timedObjective = StationObjective(abStationD)
    Int completedStage = StationCompleteStage(abStationD)
    GlobalVariable timeLimit = TW043_SecurityStationATimeLimit
    ObjectReference waitMarker = TW043_DownloadAWait
    Int waveIndex = 0
    If abStationD
        timeLimit = TW043_SecurityStationDTimeLimit
        waitMarker = TW043_DownloadDWait
        waveIndex = 1
    EndIf
    If IsStageDone(completedStage)
        Return
    EndIf

    patrol.HoldEscortAt(waitMarker)
    SetObjectiveDisplayed(timedObjective, True, True)
    StartPatrolWave(waveIndex)
    Float seconds = 90.0
    If timeLimit != None && timeLimit.GetValue() > 0.0
        seconds = timeLimit.GetValue()
    EndIf
    SetStageOnTimer(completedStage, seconds)
EndFunction

Function TryBeginStationDefense(Bool abStationD)
    If abStationD
        If IsStageDone(151) && IsStageDone(152)
            BeginStationDefense(True)
        EndIf
        Return
    EndIf
    If IsStageDone(41) && IsStageDone(42)
        BeginStationDefense(False)
    EndIf
EndFunction

Function CompleteStationDefense(Bool abStationD, ObjectReference akNextLegStart, Int aiNextArrivalStage)
    StopPatrolWaves()
    CompleteOpenObjective(StationObjective(abStationD))
    If IsPatrolOver()
        Return
    EndIf
    StartPatrolLeg(akNextLegStart, aiNextArrivalStage)
EndFunction

Function ShutdownPatrol(Bool abFailed)
    TW043QuestScript patrol = PatrolScript()
    StopPatrolWaves()
    If patrol != None
        patrol.EndPatrol()
    EndIf
    If abFailed
        FailOpenObjectives()
    EndIf
    StartTimer(5.0, 44999)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 44999
        Stop()
        Return
    EndIf
    If aiTimerID > 45000 && aiTimerID <= 46000
        Int pendingStage = aiTimerID - 45000
        If IsRunning() && !IsPatrolOver() && !IsStageDone(pendingStage)
            SetStage(pendingStage)
        EndIf
    EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
    ResetPatrolObjectives()
    SetObjectiveDisplayed(0, True, True)
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.AnnouncePatrolIntro(Alias_LC043Warden)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    If TW043_010_Start && !TW043_010_Start.IsPlaying()
        TW043_010_Start.Start()
    EndIf
    CompleteOpenObjective(0)
    SetObjectiveDisplayed(10, True, True)
    TW043QuestScript patrol = PatrolScript()
    If patrol == None
        Return
    EndIf
    patrol.RetireOldGuard()
    patrol.StartEventClock()
    patrol.StartChosenPatrolRoute()
EndFunction

Function Fragment_Stage_0011_Item_00()
    ObjectReference legStart = TW043_Patrol11Start
    If legStart == None
        legStart = TW043_Patrol10Start
    EndIf
    StartPatrolLeg(legStart, 30)
    SetStageOnTimer(20, 3.0)
EndFunction

Function Fragment_Stage_0012_Item_00()
    StartPatrolLeg(TW043_Patrol12Start, 116)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If TW043_020_AWing && !TW043_020_AWing.IsPlaying()
        TW043_020_AWing.Start()
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    If TW043_030_SecurityA && !TW043_030_SecurityA.IsPlaying()
        TW043_030_SecurityA.Start()
    EndIf
    SetStageOnTimer(31, 8.0)
EndFunction

Function Fragment_Stage_0031_Item_00()
    SetObjectiveDisplayed(StationObjective(False), True, True)
    If !IsStageDone(40) && !IsPatrolOver()
        SetStage(40)
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    If TW043_040_DownloadA && !TW043_040_DownloadA.IsPlaying()
        TW043_040_DownloadA.Start()
    EndIf
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.HoldEscortAt(TW043_DownloadAWait)
        patrol.SetStageWhenGuardLoaded(42)
    EndIf
    SetStageOnTimer(41, 10.0)
EndFunction

Function Fragment_Stage_0041_Item_00()
    TryBeginStationDefense(False)
EndFunction

Function Fragment_Stage_0042_Item_00()
    TryBeginStationDefense(False)
EndFunction

Function Fragment_Stage_0060_Item_00()
    If TW043_DownloadComplete && !TW043_DownloadComplete.IsPlaying()
        TW043_DownloadComplete.Start()
    EndIf
    CompleteStationDefense(False, TW043_Patrol60Start, 110)
    If !IsStageDone(100) && !IsPatrolOver()
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If TW043_100_BWing && !TW043_100_BWing.IsPlaying()
        TW043_100_BWing.Start()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    If TW043_110_DiningHall && !TW043_110_DiningHall.IsPlaying()
        TW043_110_DiningHall.Start()
    EndIf
    SetStageOnTimer(116, 25.0)
EndFunction

Function Fragment_Stage_0115_Item_00()
    If TW043_115_PrisonYard && !TW043_115_PrisonYard.IsPlaying()
        TW043_115_PrisonYard.Start()
    EndIf
    If IsRoute2()
        SetStageOnTimer(120, 15.0)
        Return
    EndIf
    If !IsStageDone(13)
        SetStage(13)
    EndIf
    SetStageOnTimer(200, 15.0)
EndFunction

Function Fragment_Stage_0116_Item_00()
    If IsRoute2()
        TeleportGuard(Alias_Marker_YardTeleport_Route2)
    Else
        TeleportGuard(Alias_Marker_YardTeleport_Route1)
    EndIf
    If !IsStageDone(115) && !IsPatrolOver()
        SetStage(115)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    If TW043_120_Solitary && !TW043_120_Solitary.IsPlaying()
        TW043_120_Solitary.Start()
    EndIf
    SetStageOnTimer(122, 25.0)
EndFunction

Function Fragment_Stage_0121_Item_00()
    If TW043_121_Solitary2 && !TW043_121_Solitary2.IsPlaying()
        TW043_121_Solitary2.Start()
    EndIf
    SetStageOnTimer(130, 20.0)
EndFunction

Function Fragment_Stage_0122_Item_00()
    TeleportGuard(Alias_Marker_SolitaryTeleport_Route2)
    If !IsStageDone(121) && !IsPatrolOver()
        SetStage(121)
    EndIf
EndFunction

Function Fragment_Stage_0130_Item_00()
    If TW043_130_DWing && !TW043_130_DWing.IsPlaying()
        TW043_130_DWing.Start()
    EndIf
    SetStageOnTimer(140, 20.0)
EndFunction

Function Fragment_Stage_0140_Item_00()
    If TW043_140_SecurityD && !TW043_140_SecurityD.IsPlaying()
        TW043_140_SecurityD.Start()
    EndIf
    SetStageOnTimer(141, 8.0)
EndFunction

Function Fragment_Stage_0141_Item_00()
    SetObjectiveDisplayed(StationObjective(True), True, True)
    If !IsStageDone(150) && !IsPatrolOver()
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If TW043_150_DownloadD && !TW043_150_DownloadD.IsPlaying()
        TW043_150_DownloadD.Start()
    EndIf
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.HoldEscortAt(TW043_DownloadDWait)
        patrol.SetStageWhenGuardLoaded(152)
    EndIf
    SetStageOnTimer(151, 10.0)
EndFunction

Function Fragment_Stage_0151_Item_00()
    TryBeginStationDefense(True)
EndFunction

Function Fragment_Stage_0152_Item_00()
    TryBeginStationDefense(True)
EndFunction

Function Fragment_Stage_0170_Item_00()
    If TW043_DownloadComplete && !TW043_DownloadComplete.IsPlaying()
        TW043_DownloadComplete.Start()
    EndIf
    If !IsStageDone(13)
        SetStage(13)
    EndIf
    CompleteStationDefense(True, TW043_Patrol170Start, 200)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If TW043_200_SecurityDoor && !TW043_200_SecurityDoor.IsPlaying()
        TW043_200_SecurityDoor.Start()
    EndIf
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.OpenSecurityDoors()
    EndIf
    SetStageOnTimer(210, 12.0)
EndFunction

Function Fragment_Stage_0210_Item_00()
    StartPatrolLeg(TW043_Patrol210Start, 220)
EndFunction

Function Fragment_Stage_0220_Item_00()
    If TW043_220_End && !TW043_220_End.IsPlaying()
        TW043_220_End.Start()
    EndIf
    SetStageOnTimer(230, 12.0)
EndFunction

Function Fragment_Stage_0230_Item_00()
    CompleteOpenObjective(10)
    CompleteOpenObjective(11)
    CompleteQuest()
    ShutdownPatrol(False)
EndFunction

Function Fragment_Stage_0250_Item_00()
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.AnnouncePatrolFailure(Alias_LC043Warden)
    EndIf
    StopPatrolWaves()
    FailOpenObjectives()
    If !IsStageDone(999)
        SetStage(999)
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    ShutdownPatrol(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CancelTimer(44999)
    StopPatrolWaves()
    TW043QuestScript patrol = PatrolScript()
    If patrol != None
        patrol.EndPatrol()
    EndIf
EndFunction
