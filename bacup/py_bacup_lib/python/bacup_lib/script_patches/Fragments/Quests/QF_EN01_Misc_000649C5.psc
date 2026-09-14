Function Fragment_Stage_0004_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    Alias_currentPlayer.ForceRefTo(playerRef)
    If EN01_Misc_StartedValue != None
        playerRef.SetValue(EN01_Misc_StartedValue, 1.0)
    EndIf
    If EN01_StartedValue != None
        playerRef.SetValue(EN01_StartedValue, 1.0)
    EndIf
    ObjectReference startNote = None
    If Alias_NoteObjectStory != None
        startNote = Alias_NoteObjectStory.GetRef()
    EndIf
    If startNote != None
        If Alias_NoteObject != None && Alias_NoteObject.GetRef() != startNote
            Alias_NoteObject.ForceRefTo(startNote)
        EndIf
        If !IsStageDone(5)
            SetStage(5)
        EndIf
    ElseIf !IsStageDone(6)
        SetStage(6)
    EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0006_Item_00()
    SetObjectiveDisplayed(6, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveCompleted(6, True)
    SetObjectiveDisplayed(10, True)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN01_PlayerFoundQCNotes != None
        playerRef.SetValue(EN01_PlayerFoundQCNotes, 1.0)
    EndIf
    RecordCheckpoint(1.0)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN01_PlayerKnowsSamBlackwell != None
        playerRef.SetValue(EN01_PlayerKnowsSamBlackwell, 1.0)
    EndIf
    RecordCheckpoint(2.0)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(20, True)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN01_MiscCompletedValue != None
        playerRef.SetValue(EN01_MiscCompletedValue, 1.0)
    EndIf
EndFunction

Function RecordCheckpoint(Float afCheckpointValue)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || EN01_Misc_CheckpointValue == None
        Return
    EndIf
    If playerRef.GetValue(EN01_Misc_CheckpointValue) >= afCheckpointValue
        Return
    EndIf
    playerRef.SetValue(EN01_Misc_CheckpointValue, afCheckpointValue)
    If CheckpointMessage != None
        CheckpointMessage.Show()
    EndIf
EndFunction
