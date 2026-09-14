ObjectReference Function GetCampEncounterMarker()
    ObjectReference markerRef = SpawnedSign.GetReference()
    If markerRef == None
        markerRef = CAMPObj.GetReference()
    EndIf
    If markerRef == None
        markerRef = owningPlayer.GetReference()
    EndIf
    Return markerRef
EndFunction

Actor Function SpawnEncounterActor(Form actorToSpawn)
    ObjectReference markerRef = GetCampEncounterMarker()
    If markerRef && actorToSpawn
        Return markerRef.PlaceAtMe(actorToSpawn, 1, True, False, False) as Actor
    EndIf
    Return None
EndFunction

Function SpawnFirstEncounter()
    If FirstEncNPC.GetReference()
        Return
    EndIf
    Form visitorBase = None
    If FirstEncLists && FirstEncLists.Length > 0 && FirstEncLists[0] && FirstEncLists[0].GetSize() > 0
        visitorBase = FirstEncLists[0].GetAt(0)
    EndIf
    If visitorBase == None
        visitorBase = W05_MQ_002_Radical_RadicalGanger01
    EndIf
    Actor visitorRef = SpawnEncounterActor(visitorBase)
    If visitorRef
        FirstEncNPC.ForceRefTo(visitorRef)
        visitorRef.EvaluatePackage()
    EndIf
    If !IsStageDone(iFirstEncSpawnedStage)
        SetStage(iFirstEncSpawnedStage)
    EndIf
    If !IsStageDone(498)
        SetStage(498)
    EndIf
EndFunction

Function SpawnSecondEncounter()
    If bSecondEncOnce
        Return
    EndIf
    bSecondEncOnce = True
    Actor ganger01 = RadGanger01.GetActorReference()
    Actor ganger02 = RadGanger02.GetActorReference()
    If ganger01 == None
        ganger01 = SpawnEncounterActor(W05_MQ_002_Radical_RadicalGanger01)
        If ganger01
            RadGanger01.ForceRefTo(ganger01)
            RadGanger01QGhosted.ForceRefTo(ganger01)
        EndIf
    EndIf
    If ganger02 == None
        ganger02 = SpawnEncounterActor(W05_MQ_002_Radical_RadicalGanger02)
        If ganger02
            RadGanger02.ForceRefTo(ganger02)
            RadGanger02QGhosted.ForceRefTo(ganger02)
        EndIf
    EndIf
    If ganger01
        ganger01.EvaluatePackage()
    EndIf
    If ganger02
        ganger02.EvaluatePackage()
    EndIf
    If !IsStageDone(iGangersSceneTriggeredStage)
        SetStage(iGangersSceneTriggeredStage)
    EndIf
EndFunction

Function TryStartMuscleQuest()
    Quest muscleQuest = Game.GetFormFromFile(0x0041A39D, "SeventySix.esm") as Quest
    If muscleQuest == None || muscleQuest.IsRunning() || muscleQuest.IsCompleted()
        Return
    EndIf

    Actor playerRef = owningPlayer.GetActorReference()
    If playerRef == None || playerRef.IsInScene()
        StartTimer(1.0, 8950)
        Return
    EndIf

    Keyword startKeyword = Game.GetFormFromFile(0x0041A340, "SeventySix.esm") as Keyword
    If startKeyword != None
        startKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If muscleQuest.IsRunning() || muscleQuest.IsCompleted()
        Return
    EndIf

    StartTimer(1.0, 8950)
EndFunction

Event OnQuestInit()
    bProcessing = False
    bProcessingIncrement = False
    bSecondEncOnce = False
    If GetCurrentStageID() == iFirstTimeSceneStage && !IsStageDone(103) && !IsStageDone(105)
        StartTimer(0.5, 2103)
    EndIf
EndEvent

Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == iFirstTimeSceneStage
        StartTimer(0.5, 2103)
    ElseIf auiStageID == 103 || auiStageID == 105
        ; 103/105 are the post-intro checkpoints the 2103 timer lands on once the
        ; player leaves the Duchess scene, but FO76 drove everything past them from
        ; server-stripped fragments, so 110/125/150 have no remaining trigger and
        ; the quest parks on objective 100. Stage 110 puts the sign plans in the
        ; player's pack (objective 110 reads them from inventory) and 150 closes
        ; objective 100; without both, obj 110 would display with nothing to read.
        If !IsStageDone(110)
            SetStage(110)
        EndIf
        If !IsStageDone(125)
            SetStage(125)
        EndIf
        If !IsStageDone(150)
            SetStage(150)
        EndIf
    ElseIf auiStageID == iPlayerApproachedCAMP
        StartTimer(1.0, iFirstEncTimerID)
    ElseIf auiStageID == iGangersSpawnedStage
        SpawnSecondEncounter()
    ElseIf auiStageID == 745 && !IsStageDone(iRadGangersPlayerEnemyStage)
        StartTimer(iPlayerEnemyTimerLength, iPlayerEnemyTimerID)
    ElseIf auiStageID == iRadGangersPlayerEnemyStage
        StartTimer(1.0, 746)
    ElseIf auiStageID == 1575
        StartTimer(1.0, 1575)
    ElseIf auiStageID == 8950
        TryStartMuscleQuest()
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == 2103
        Actor playerRef = owningPlayer.GetActorReference()
        If playerRef && playerRef.IsInScene()
            StartTimer(0.5, 2103)
        ElseIf !IsStageDone(103) && !IsStageDone(105)
            SetStage(103)
        EndIf
    ElseIf aiTimerID == iFirstEncTimerID
        SpawnFirstEncounter()
    ElseIf aiTimerID == iSecondEncTimerID
        SpawnSecondEncounter()
    ElseIf aiTimerID == iPlayerEnemyTimerID && !IsStageDone(iRadGangersPlayerEnemyStage)
        SetStage(iRadGangersPlayerEnemyStage)
    ElseIf aiTimerID == 746
        Actor ganger01 = RadGanger01.GetActorReference()
        Actor ganger02 = RadGanger02.GetActorReference()
        If ganger01 && ganger01.IsDead() && !IsStageDone(765)
            SetStage(765)
        EndIf
        If ganger02 && ganger02.IsDead() && !IsStageDone(766)
            SetStage(766)
        EndIf
        If IsStageDone(765) && IsStageDone(766)
            If !IsStageDone(799)
                SetStage(799)
            EndIf
        ElseIf IsStageDone(iRadGangersPlayerEnemyStage)
            StartTimer(1.0, 746)
        EndIf
    ElseIf aiTimerID == 1575
        ReferenceAlias roperAlias = GetAlias(17) as ReferenceAlias
        Actor roperRef = None
        If roperAlias
            roperRef = roperAlias.GetActorReference()
        EndIf
        If roperRef == None || roperRef.IsDead()
            If !IsStageDone(1600)
                SetStage(1600)
            EndIf
        ElseIf IsStageDone(1575) && !IsStageDone(1600)
            StartTimer(1.0, 1575)
        EndIf
    ElseIf aiTimerID == 8950
        TryStartMuscleQuest()
    EndIf
EndEvent
