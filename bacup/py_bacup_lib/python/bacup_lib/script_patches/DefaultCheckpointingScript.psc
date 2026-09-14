Event OnQuestInit()
    If CheckpointingInProgress
        Return
    EndIf

    CheckpointingInProgress = True
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && CheckpointAV != None
        currentCheckpointValue = playerRef.GetValue(CheckpointAV) as Int
        Int selectedIndex = FindCheckpointStageIndex(currentCheckpointValue)
        If selectedIndex >= 0
            Int targetStage = CheckpointStages[selectedIndex].stageToSet
            If !IsStageDone(targetStage)
                SetStage(targetStage)
            EndIf
            If CheckpointStages[selectedIndex].showMessage && CheckpointMessage != None
                CheckpointMessage.Show()
            EndIf
        EndIf
    EndIf
    CheckpointingInProgress = False
EndEvent

Int Function FindCheckpointStageIndex(Int aiCheckpointValue)
    If CheckpointStages == None || CheckpointStages.Length == 0
        Return -1
    EndIf

    Int selectedIndex = -1
    Int index = 0
    While index < CheckpointStages.Length
        If CheckpointStages[index].checkpointValue <= aiCheckpointValue
            If selectedIndex < 0 || CheckpointStages[index].checkpointValue > CheckpointStages[selectedIndex].checkpointValue
                selectedIndex = index
            EndIf
        EndIf
        index += 1
    EndWhile
    Return selectedIndex
EndFunction
