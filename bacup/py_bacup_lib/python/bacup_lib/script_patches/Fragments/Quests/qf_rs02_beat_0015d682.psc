Int[] Function EventObjectives()
    Int[] objectives = New Int[8]
    objectives[0] = 10
    objectives[1] = 15
    objectives[2] = 20
    objectives[3] = 30
    objectives[4] = 40
    objectives[5] = 50
    objectives[6] = 60
    objectives[7] = 70
    Return objectives
EndFunction

Function ResetEventObjectives()
    Int[] objectives = EventObjectives()
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function HideEventObjectives()
    Int[] objectives = EventObjectives()
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

DefaultQuestEncounterWaveScript Function WaveSystem()
    Return (Self as Quest) as DefaultQuestEncounterWaveScript
EndFunction

Function StartGhoulWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopGhoulWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StopAllGhoulWaves()
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StopAllEncounterWaves(True)
    EndIf
EndFunction

Function PlayAlarm(ObjectReference akSource)
    If OBJAlarmALP != None && akSource != None
        OBJAlarmALP.Play(akSource)
    EndIf
EndFunction

Function SetHoldPositionArea(ReferenceAlias akArea)
    If Alias_CurrentHoldPositionArea == None || akArea == None
        Return
    EndIf
    ObjectReference areaRef = akArea.GetReference()
    If areaRef != None
        Alias_CurrentHoldPositionArea.ForceRefTo(areaRef)
    EndIf
EndFunction

Function UnlockStashDoor(ReferenceAlias akDoor)
    If akDoor == None
        Return
    EndIf
    ObjectReference doorRef = akDoor.GetReference()
    If doorRef != None
        doorRef.Unlock()
    EndIf
EndFunction

Function LockStashDoor(ReferenceAlias akDoor)
    If akDoor == None
        Return
    EndIf
    ObjectReference doorRef = akDoor.GetReference()
    If doorRef != None
        doorRef.Lock(True)
    EndIf
EndFunction

Function LockStashDoors()
    LockStashDoor(Alias_StashRoom1Door)
    LockStashDoor(Alias_StashRoom2Door)
    LockStashDoor(Alias_StashRoom3Door)
EndFunction

Function SetKickoutTriggersEnabled(Bool abEnabled)
    If abEnabled
        If RS02_Beat_Kickout1EnableMarker != None
            RS02_Beat_Kickout1EnableMarker.Enable(False)
        EndIf
        If RS02_Beat_StashRoom2KickoutTrigger != None
            RS02_Beat_StashRoom2KickoutTrigger.Enable(False)
        EndIf
        If RS02_Beat_StashRoom3KickoutTrigger != None
            RS02_Beat_StashRoom3KickoutTrigger.Enable(False)
        EndIf
    Else
        If RS02_Beat_Kickout1EnableMarker != None
            RS02_Beat_Kickout1EnableMarker.Disable(False)
        EndIf
        If RS02_Beat_StashRoom2KickoutTrigger != None
            RS02_Beat_StashRoom2KickoutTrigger.Disable(False)
        EndIf
        If RS02_Beat_StashRoom3KickoutTrigger != None
            RS02_Beat_StashRoom3KickoutTrigger.Disable(False)
        EndIf
    EndIf
EndFunction

Function SetRadioTransmitterEnabled(Bool abEnabled)
    If Alias_RadioTransmitter == None
        Return
    EndIf
    ObjectReference transmitterRef = Alias_RadioTransmitter.GetReference()
    If transmitterRef == None
        Return
    EndIf
    If abEnabled
        transmitterRef.Enable(False)
    Else
        transmitterRef.Disable(False)
    EndIf
EndFunction

Function ReturnSteelheartToPod()
    If Alias_Steelheart == None || Alias_SteelheartPod == None
        Return
    EndIf
    Actor steelheartRef = Alias_Steelheart.GetActorReference()
    ObjectReference podRef = Alias_SteelheartPod.GetReference()
    If steelheartRef == None || podRef == None
        Return
    EndIf
    If steelheartRef.IsDisabled()
        steelheartRef.Enable(False)
    EndIf
    If steelheartRef.GetDistance(podRef) > 1500.0
        steelheartRef.MoveTo(podRef)
    EndIf
EndFunction

Function StopPatrolScenes()
    If RS02_Beat_StartPatrol != None && RS02_Beat_StartPatrol.IsPlaying()
        RS02_Beat_StartPatrol.Stop()
    EndIf
    If RS02_Beat_Loc1AlarmScene != None && RS02_Beat_Loc1AlarmScene.IsPlaying()
        RS02_Beat_Loc1AlarmScene.Stop()
    EndIf
    If RS02_Beat_Loc2TravelScene != None && RS02_Beat_Loc2TravelScene.IsPlaying()
        RS02_Beat_Loc2TravelScene.Stop()
    EndIf
    If RS02_Beat_Loc2AlarmScene != None && RS02_Beat_Loc2AlarmScene.IsPlaying()
        RS02_Beat_Loc2AlarmScene.Stop()
    EndIf
    If RS02_Beat_Loc3TravelScene != None && RS02_Beat_Loc3TravelScene.IsPlaying()
        RS02_Beat_Loc3TravelScene.Stop()
    EndIf
    If RS02_Beat_Loc3AlarmScene != None && RS02_Beat_Loc3AlarmScene.IsPlaying()
        RS02_Beat_Loc3AlarmScene.Stop()
    EndIf
    If RS02_Beat_LocFinalTravelScene != None && RS02_Beat_LocFinalTravelScene.IsPlaying()
        RS02_Beat_LocFinalTravelScene.Stop()
    EndIf
EndFunction

Function FailOpenEscortObjectives()
    Int[] objectives = EventObjectives()
    Int index = 0
    While index < objectives.Length
        Int objective = objectives[index]
        If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
            SetObjectiveFailed(objective, True)
        EndIf
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResetEventObjectives()
    LockStashDoors()
    SetKickoutTriggersEnabled(False)
    SetRadioTransmitterEnabled(True)
    ReturnSteelheartToPod()
    If RS02_Beat_Running != None
        RS02_Beat_Running.SetValue(1.0)
    EndIf
    If RS02_Beat_RadioScene != None && !RS02_Beat_RadioScene.IsPlaying()
        RS02_Beat_RadioScene.Start()
    EndIf
    SetObjectiveDisplayed(10, True, True)
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsObjectiveCompleted(10)
        SetObjectiveDisplayed(10, True, True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(15, True, True)
    SetHoldPositionArea(Alias_HoldPositionArea01)
    If RS02_Beat_StartPatrol && !RS02_Beat_StartPatrol.IsPlaying()
        RS02_Beat_StartPatrol.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    StartGhoulWave("Mid 1a")
EndFunction

Function Fragment_Stage_0275_Item_00()
    StartGhoulWave("Mid 1b")
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(15, True)
    StopGhoulWave("Mid 1a")
    StopGhoulWave("Mid 1b")
    SetHoldPositionArea(Alias_HoldPositionArea01)
    If RS02_Beat_Loc1AlarmScene && !RS02_Beat_Loc1AlarmScene.IsPlaying()
        RS02_Beat_Loc1AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(20, True, True)
    PlayAlarm(RS02_Beat_AlarmSoundMarkerLoc1Ref)
    PlayAlarm(RS02_Beat_Loc1Speaker)
    StartGhoulWave("Loc 1")
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(20, True)
    StopGhoulWave("Loc 1")
    UnlockStashDoor(Alias_StashRoom1Door)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(30, True, True)
    SetHoldPositionArea(Alias_HoldPositionArea02)
    If RS02_Beat_Loc2TravelScene && !RS02_Beat_Loc2TravelScene.IsPlaying()
        RS02_Beat_Loc2TravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    StartGhoulWave("Mid 2")
EndFunction

Function Fragment_Stage_0675_Item_00()
    StartGhoulWave("Mid 2b")
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(30, True)
    StopGhoulWave("Mid 2")
    StopGhoulWave("Mid 2b")
    SetHoldPositionArea(Alias_HoldPositionArea02)
    If RS02_Beat_Loc2AlarmScene && !RS02_Beat_Loc2AlarmScene.IsPlaying()
        RS02_Beat_Loc2AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(40, True, True)
    PlayAlarm(RS02_Beat_AlarmSoundMarkerLoc2Ref)
    StartGhoulWave("Loc 2")
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(40, True)
    StopGhoulWave("Loc 2")
    UnlockStashDoor(Alias_StashRoom2Door)
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(50, True, True)
    SetHoldPositionArea(Alias_HoldPositionArea03)
    If RS02_Beat_Loc3TravelScene && !RS02_Beat_Loc3TravelScene.IsPlaying()
        RS02_Beat_Loc3TravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1050_Item_00()
    StartGhoulWave("Mid 3")
EndFunction

Function Fragment_Stage_1075_Item_00()
    StartGhoulWave("Mid 3b")
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(50, True)
    StopGhoulWave("Mid 3")
    StopGhoulWave("Mid 3b")
    SetHoldPositionArea(Alias_HoldPositionArea03)
    If RS02_Beat_Loc3AlarmScene && !RS02_Beat_Loc3AlarmScene.IsPlaying()
        RS02_Beat_Loc3AlarmScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveDisplayed(60, True, True)
    PlayAlarm(RS02_Beat_AlarmSoundMarkerLoc3Ref)
    StartGhoulWave("Loc 3")
EndFunction

Function Fragment_Stage_1301_Item_00()
    SetObjectiveCompleted(60, True)
    StopGhoulWave("Loc 3")
    UnlockStashDoor(Alias_StashRoom3Door)
    If !IsStageDone(1401)
        SetStage(1401)
    EndIf
EndFunction

Function Fragment_Stage_1401_Item_00()
    SetObjectiveDisplayed(70, True, True)
    SetHoldPositionArea(Alias_HoldPositionArea04)
    If RS02_Beat_LocFinalTravelScene && !RS02_Beat_LocFinalTravelScene.IsPlaying()
        RS02_Beat_LocFinalTravelScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_1450_Item_00()
    StartGhoulWave("Mid 4")
EndFunction

Function Fragment_Stage_5000_Item_00()
    If FailStage >= 0 && IsStageDone(FailStage)
        Return
    EndIf
    SetObjectiveCompleted(70, True)
    StopAllGhoulWaves()
    If !IsStageDone(5750)
        SetStage(5750)
    EndIf
EndFunction

Function Fragment_Stage_5500_Item_00()
    StopAllGhoulWaves()
    StopPatrolScenes()
    FailOpenEscortObjectives()
    If Scene_QuestFail && !Scene_QuestFail.IsPlaying()
        Scene_QuestFail.Start()
    EndIf
    If !IsStageDone(5750)
        SetStage(5750)
    EndIf
EndFunction

Function Fragment_Stage_5750_Item_00()
    StopAllGhoulWaves()
    SetKickoutTriggersEnabled(True)
EndFunction

Function Fragment_Stage_6000_Item_00()
    StopAllGhoulWaves()
    StopPatrolScenes()
    LockStashDoors()
    SetRadioTransmitterEnabled(False)
    If RS02_Beat_Running != None
        RS02_Beat_Running.SetValue(0.0)
    EndIf
    HideEventObjectives()
    Stop()
EndFunction
