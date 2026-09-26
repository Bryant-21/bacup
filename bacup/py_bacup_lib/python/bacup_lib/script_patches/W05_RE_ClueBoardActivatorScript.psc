Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer == None || activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    PlayerToCheck = activatingPlayer
    ThisPlayersQuest = W05_MQ_000P
    EntranceTrigger = GetLinkedRef(W05_RE_EntranceTrigger_Keyword)
    ClueEnableMarker01 = GetLinkedRef(W05_RE_ClueEnableMarker01_Keyword)
    ClueEnableMarker02 = GetLinkedRef(W05_RE_ClueEnableMarker02_Keyword)
    ClueEnableMarker03 = GetLinkedRef(W05_RE_ClueEnableMarker03_Keyword)
    ClueEnableMarker04 = GetLinkedRef(W05_RE_ClueEnableMarker04_Keyword)
    ClueEnableMarker05 = GetLinkedRef(W05_RE_ClueEnableMarker05_Keyword)

    PlaceReadyEvidence(activatingPlayer, W05_Clue1_ActorValue, 150, ClueEnableMarker01)
    PlaceReadyEvidence(activatingPlayer, W05_Clue2_ActorValue, 250, ClueEnableMarker02)
    PlaceReadyEvidence(activatingPlayer, W05_Clue3_ActorValue, 350, ClueEnableMarker03)
    PlaceReadyEvidence(activatingPlayer, W05_Clue4_ActorValue, 450, ClueEnableMarker04)
    PlaceReadyEvidence(activatingPlayer, W05_Clue5_ActorValue, 550, ClueEnableMarker05)

    Bool allEvidencePlaced = W05_Clue1_ActorValue != None
    allEvidencePlaced = allEvidencePlaced && W05_Clue2_ActorValue != None
    allEvidencePlaced = allEvidencePlaced && W05_Clue3_ActorValue != None
    allEvidencePlaced = allEvidencePlaced && W05_Clue4_ActorValue != None
    allEvidencePlaced = allEvidencePlaced && W05_Clue5_ActorValue != None
    If allEvidencePlaced
        allEvidencePlaced = activatingPlayer.GetValue(W05_Clue1_ActorValue) >= 2.0
        allEvidencePlaced = allEvidencePlaced && activatingPlayer.GetValue(W05_Clue2_ActorValue) >= 2.0
        allEvidencePlaced = allEvidencePlaced && activatingPlayer.GetValue(W05_Clue3_ActorValue) >= 2.0
        allEvidencePlaced = allEvidencePlaced && activatingPlayer.GetValue(W05_Clue4_ActorValue) >= 2.0
        allEvidencePlaced = allEvidencePlaced && activatingPlayer.GetValue(W05_Clue5_ActorValue) >= 2.0
    EndIf
    If ThisPlayersQuest != None && !ThisPlayersQuest.IsStageDone(999) && allEvidencePlaced
        ThisPlayersQuest.SetStage(999)
    EndIf
EndEvent

Function PlaceReadyEvidence(Actor playerRef, ActorValue clueValue, Int placedStage, ObjectReference clueMarker)
    If playerRef == None || clueValue == None
        Return
    EndIf

    If playerRef.GetValue(clueValue) == 1.0
        If ThisPlayersQuest != None && !ThisPlayersQuest.IsStageDone(placedStage)
            ThisPlayersQuest.SetStage(placedStage)
        EndIf
        playerRef.SetValue(clueValue, 2.0)
    EndIf

    If clueMarker != None && playerRef.GetValue(clueValue) >= 2.0 && clueMarker.IsDisabled()
        clueMarker.Enable()
    EndIf
EndFunction
