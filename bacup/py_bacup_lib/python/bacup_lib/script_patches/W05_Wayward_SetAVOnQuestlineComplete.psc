Event OnTriggerEnter(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || W05_MQ_004P_Crane == None
        Return
    EndIf
    If !W05_MQ_004P_Crane.IsCompleted() || W05_Wayward_PlayerCompletedQuestline == None
        Return
    EndIf

    Bool firstCompletedEntry = playerRef.GetValue(W05_Wayward_PlayerCompletedQuestline) < 1.0
    playerRef.SetValue(W05_Wayward_PlayerCompletedQuestline, 1.0)
    If W05_MQ_003P_Muscle_Wayward_PollyHeadOn != None
        playerRef.SetValue(W05_MQ_003P_Muscle_Wayward_PollyHeadOn, 0.0)
    EndIf
    If W05_MQ_003P_Muscle_EmptyJugSwap != None
        playerRef.SetValue(W05_MQ_003P_Muscle_EmptyJugSwap, 1.0)
    EndIf
    If W05_MQ_003P_Muscle_SolChairSwap != None
        playerRef.SetValue(W05_MQ_003P_Muscle_SolChairSwap, 1.0)
    EndIf
    Bool badEndingActive = B21_WaywardState.BadEndingActive(playerRef, W05_MQ_004P_Crane_BadEnding)
    If !badEndingActive && W05_Wayward_PlayerVendorInteractChoicePerk != None
        If !playerRef.HasPerk(W05_Wayward_PlayerVendorInteractChoicePerk)
            playerRef.AddPerk(W05_Wayward_PlayerVendorInteractChoicePerk)
        EndIf
    EndIf
    If firstCompletedEntry && !badEndingActive
        W05_Wayward_ExtDialogueScript exteriorDialogue = W05_DialogueTheWayward_Exterior as W05_Wayward_ExtDialogueScript
        If exteriorDialogue != None
            exteriorDialogue.PlayWrapUpBroadcast(W05_Wayward_WrapUpBroadcast, playerRef)
        EndIf
    EndIf
EndEvent
