Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    Quest successorQuest = Game.GetFormFromFile(0x005F3CF5, "SeventySix.esm") as Quest
    If owningQuest == None || playerRef == None
        Return
    EndIf

    If playerRef.GetCurrentLocation() == Loc_Tunnel
        If owningQuest.IsStageDone(Stage_VinesReadyForDestruction)
            playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        EndIf
    Else
        playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        If owningQuest.IsStageDone(Stage_QuestCompleted) && successorQuest != None && (successorQuest.IsRunning() || successorQuest.IsCompleted()) && !owningQuest.IsStageDone(StageToSet_CleanUpQuest)
            owningQuest.SetStage(StageToSet_CleanUpQuest)
        EndIf
    EndIf
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    Quest successorQuest = Game.GetFormFromFile(0x005F3CF5, "SeventySix.esm") as Quest
    If owningQuest == None || playerRef == None
        Return
    EndIf

    If akNewLoc == Loc_Tunnel
        If owningQuest.IsStageDone(Stage_VinesReadyForDestruction)
            playerRef.AddKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        EndIf
    Else
        playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        If owningQuest.IsStageDone(Stage_QuestCompleted) && successorQuest != None && (successorQuest.IsRunning() || successorQuest.IsCompleted()) && !owningQuest.IsStageDone(StageToSet_CleanUpQuest)
            owningQuest.SetStage(StageToSet_CleanUpQuest)
        EndIf
    EndIf
EndEvent

Function ReconcileSuccessorAcceptance()
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    Quest successorQuest = Game.GetFormFromFile(0x005F3CF5, "SeventySix.esm") as Quest
    If owningQuest == None || playerRef == None || successorQuest == None
        Return
    EndIf

    If owningQuest.IsStageDone(Stage_QuestCompleted) && (successorQuest.IsRunning() || successorQuest.IsCompleted()) && playerRef.GetCurrentLocation() != Loc_Tunnel && !owningQuest.IsStageDone(StageToSet_CleanUpQuest)
        playerRef.RemoveKeyword(BS02_MQ03_Blue_AllowVineDestroy)
        owningQuest.SetStage(StageToSet_CleanUpQuest)
    EndIf
EndFunction
