Event OnQuestInit()
    PlayerRef = myPlayer.GetActorReference()
    photoCount = 0
    plantCounter = 0
EndEvent

Function SelectPlantRegions()
    Int firstRegion = Utility.RandomInt(0, 6)
    Int secondRegion = Utility.RandomInt(0, 6)
    While secondRegion == firstRegion
        secondRegion = Utility.RandomInt(0, 6)
    EndWhile

    SetStage(200 + firstRegion)
    SetStage(200 + secondRegion)
EndFunction

Function SelectPhotoObjectives()
    If PhotoObjectives == None || PhotoObjectives.Length == 0
        totalPhotos = 0
        Return
    EndIf

    PhotoObjectivesChosen = new Int[3]
    Int slot = 0
    While slot < PhotoObjectivesChosen.Length
        PhotoObjectivesChosen[slot] = -1
        slot += 1
    EndWhile

    Int attempts = 0
    Int selectedCount = 0
    While selectedCount < totalPhotos && attempts < PhotoObjectives.Length * 8
        Int candidateIndex = Utility.RandomInt(0, PhotoObjectives.Length - 1)
        PhotoObjectiveData candidate = PhotoObjectives[candidateIndex]
        If candidate.PhotoRef != None && PhotoObjectivesChosen.Find(candidateIndex) < 0
            PhotoObjectivesChosen[selectedCount] = candidateIndex
            SetObjectiveDisplayed(candidate.Photo_ObjectiveObjective)
            selectedCount += 1
        EndIf
        attempts += 1
    EndWhile
    totalPhotos = selectedCount
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        CheckPhotoObjectives()
        CheckTaskCompletion()
        If IsRunning() && !IsStageDone(2000)
            StartTimer(2.0, 1)
        EndIf
    EndIf
EndEvent

Function CheckPhotoObjectives()
    If PlayerRef == None || PhotoObjectivesChosen == None
        Return
    EndIf

    Int slot = 0
    While slot < PhotoObjectivesChosen.Length
        Int objectiveIndex = PhotoObjectivesChosen[slot]
        If objectiveIndex >= 0
            PhotoObjectiveData objectiveData = PhotoObjectives[objectiveIndex]
            If objectiveData.PhotoRef != None && PlayerRef.GetDistance(objectiveData.PhotoRef) <= 768.0
                SetObjectiveCompleted(objectiveData.Photo_ObjectiveObjective)
                If objectiveData.PhotoObjective_CompletionStage >= 0 && !IsStageDone(objectiveData.PhotoObjective_CompletionStage)
                    SetStage(objectiveData.PhotoObjective_CompletionStage)
                EndIf
                PhotoObjectivesChosen[slot] = -1
                photoCount += 1
            EndIf
        EndIf
        slot += 1
    EndWhile
EndFunction

Function CheckTaskCompletion()
    plantCounter = 0
    If IsStageDone(800)
        plantCounter += 1
    EndIf
    If IsStageDone(850)
        plantCounter += 1
    EndIf
    If IsStageDone(900)
        plantCounter += 1
    EndIf
    If IsStageDone(950)
        plantCounter += 1
    EndIf
    If IsStageDone(1000)
        plantCounter += 1
    EndIf
    If IsStageDone(1050)
        plantCounter += 1
    EndIf
    If IsStageDone(1100)
        plantCounter += 1
    EndIf

    If plantCounter >= totalPlants && photoCount >= totalPhotos && !IsStageDone(2000)
        SetStage(2000)
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        SetObjectiveDisplayed(2)
    ElseIf auiStageID == 104
        SetObjectiveCompleted(2)
        SelectPlantRegions()
        SelectPhotoObjectives()
        CancelTimer(1)
        StartTimer(2.0, 1)
    ElseIf auiStageID == 200
        SetObjectiveDisplayed(10)
    ElseIf auiStageID == 201
        SetObjectiveDisplayed(60)
    ElseIf auiStageID == 202
        SetObjectiveDisplayed(110)
    ElseIf auiStageID == 203
        SetObjectiveDisplayed(260)
    ElseIf auiStageID == 204
        SetObjectiveDisplayed(160)
    ElseIf auiStageID == 205
        SetObjectiveDisplayed(310)
    ElseIf auiStageID == 206
        SetObjectiveDisplayed(210)
    ElseIf auiStageID == 800
        SetObjectiveCompleted(10)
    ElseIf auiStageID == 850
        SetObjectiveCompleted(60)
    ElseIf auiStageID == 900
        SetObjectiveCompleted(110)
    ElseIf auiStageID == 950
        SetObjectiveCompleted(260)
    ElseIf auiStageID == 1000
        SetObjectiveCompleted(160)
    ElseIf auiStageID == 1100
        SetObjectiveCompleted(310)
    ElseIf auiStageID == 1050
        SetObjectiveCompleted(210)
    ElseIf auiStageID == 2000
        SetObjectiveDisplayed(500)
    ElseIf auiStageID == 9000
        CompleteAllObjectives()
        CancelTimer(1)
    ElseIf auiStageID == 9990 || auiStageID == 10000
        Stop()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
EndEvent
