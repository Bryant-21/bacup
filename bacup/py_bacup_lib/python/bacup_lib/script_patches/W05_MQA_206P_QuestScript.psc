; Restores the quest-level behaviour that FO76 stripped from the client build of
; W05_MQA_206P ("Secrets Revealed").
;
; Two orphaned responsibilities live here and nowhere else:
;   * ActorSetUpData is a faction/world-state gated placement table with no other
;     consumer.  It is replayed on the checkpoint-restore and initial-setup
;     stages (1 / 5 / 6) so the cast stands where the quest expects it.
;   * Stages 5100 and 5101 ("Johnny turns on the player" / "start Johnny's
;     robbery package") carry no VMAD fragment entry, so the quest fragment
;     script cannot host them.

Event OnStageSet(int auiStageID, int auiItemID)
    If auiStageID == 1 || auiStageID == 5 || auiStageID == 6
        SetUpQuestActors()
    ElseIf auiStageID == 5100
        BeginJohnnyRobbery()
    ElseIf auiStageID == 5101
        SeatJohnnyForRobbery()
    EndIf
EndEvent

Function SetUpQuestActors()
    If ActorSetUpData == None || Alias_Player == None
        Return
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None
        Return
    EndIf
    Int i = 0
    While i < ActorSetUpData.Length
        ActorSetUpDatum datum = ActorSetUpData[i]
        If MatchesSetUpRequirements(playerRef, datum)
            PlaceSetUpActor(datum)
        EndIf
        i += 1
    EndWhile
EndFunction

Bool Function MatchesSetUpRequirements(Actor akPlayer, ActorSetUpDatum aDatum)
    If aDatum.AVRequirement1 != None && akPlayer.GetValue(aDatum.AVRequirement1) != aDatum.RequiredValue1
        Return False
    EndIf
    If aDatum.AVRequirement2 != None && akPlayer.GetValue(aDatum.AVRequirement2) != aDatum.RequiredValue2
        Return False
    EndIf
    Return True
EndFunction

Function PlaceSetUpActor(ActorSetUpDatum aDatum)
    If aDatum.ActorToSetUp == None || aDatum.OperationsRoomMarker == None
        Return
    EndIf
    Actor npcRef = aDatum.ActorToSetUp.GetActorReference()
    ObjectReference markerRef = aDatum.OperationsRoomMarker.GetReference()
    If npcRef == None || markerRef == None
        Return
    EndIf
    npcRef.MoveTo(markerRef)
    npcRef.EvaluatePackage()
EndFunction

Function BeginJohnnyRobbery()
    If Alias_Player != None
        Actor playerRef = Alias_Player.GetActorReference()
        If playerRef != None && W05_MQA_206P_MustDealWithJohnnyAV != None
            playerRef.SetValue(W05_MQA_206P_MustDealWithJohnnyAV, 1.0)
        EndIf
    EndIf
    If Johnny != None
        Actor johnnyRef = Johnny.GetActorReference()
        If johnnyRef != None
            If Alias_JohnnyGoldMarker != None
                ObjectReference goldMarker = Alias_JohnnyGoldMarker.GetReference()
                If goldMarker != None
                    johnnyRef.MoveTo(goldMarker)
                EndIf
            EndIf
            johnnyRef.EvaluatePackage()
        EndIf
    EndIf
    If !IsStageDone(5101)
        SetStage(5101)
    EndIf
EndFunction

Function SeatJohnnyForRobbery()
    If Johnny == None
        Return
    EndIf
    Actor johnnyRef = Johnny.GetActorReference()
    If johnnyRef == None
        Return
    EndIf
    If JohnnyRobberyFurniture != None
        ObjectReference furnitureRef = JohnnyRobberyFurniture.GetReference()
        If furnitureRef != None
            johnnyRef.SnapIntoInteraction(furnitureRef)
        EndIf
    EndIf
    johnnyRef.EvaluatePackage()
EndFunction
