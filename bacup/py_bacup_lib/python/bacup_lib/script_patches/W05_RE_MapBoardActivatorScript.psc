Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer == None || activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    PlayerToCheck = activatingPlayer
    ThisPlayersQuest = W05_MQ_000P
    EntranceTrigger = GetLinkedRef(W05_RE_EntranceTrigger_Keyword)
    If MapMasterDummyMarker == None
        MapMasterDummyMarker = GetLinkedRef(W05_RE_MapMasterDummyMarker_Keyword) as W05_RE_MapMasterDummyScript
    EndIf

    PlaceReadyFragment(activatingPlayer, W05_Map1_ActorValue, 1150, 1)
    PlaceReadyFragment(activatingPlayer, W05_Map2_ActorValue, 1250, 2)
    PlaceReadyFragment(activatingPlayer, W05_Map3_ActorValue, 1350, 3)
    PlaceReadyFragment(activatingPlayer, W05_Map4_ActorValue, 1450, 4)
    PlaceReadyFragment(activatingPlayer, W05_Map5_ActorValue, 1550, 5)
    PlaceReadyFragment(activatingPlayer, W05_Map6_ActorValue, 1650, 6)

    Bool allFragmentsPlaced = W05_Map1_ActorValue != None
    allFragmentsPlaced = allFragmentsPlaced && W05_Map2_ActorValue != None
    allFragmentsPlaced = allFragmentsPlaced && W05_Map3_ActorValue != None
    allFragmentsPlaced = allFragmentsPlaced && W05_Map4_ActorValue != None
    allFragmentsPlaced = allFragmentsPlaced && W05_Map5_ActorValue != None
    allFragmentsPlaced = allFragmentsPlaced && W05_Map6_ActorValue != None
    If allFragmentsPlaced
        allFragmentsPlaced = activatingPlayer.GetValue(W05_Map1_ActorValue) >= 2.0
        allFragmentsPlaced = allFragmentsPlaced && activatingPlayer.GetValue(W05_Map2_ActorValue) >= 2.0
        allFragmentsPlaced = allFragmentsPlaced && activatingPlayer.GetValue(W05_Map3_ActorValue) >= 2.0
        allFragmentsPlaced = allFragmentsPlaced && activatingPlayer.GetValue(W05_Map4_ActorValue) >= 2.0
        allFragmentsPlaced = allFragmentsPlaced && activatingPlayer.GetValue(W05_Map5_ActorValue) >= 2.0
        allFragmentsPlaced = allFragmentsPlaced && activatingPlayer.GetValue(W05_Map6_ActorValue) >= 2.0
    EndIf
    If ThisPlayersQuest != None && !ThisPlayersQuest.IsStageDone(1999) && allFragmentsPlaced
        ThisPlayersQuest.SetStage(1999)
    EndIf
EndEvent

Function PlaceReadyFragment(Actor playerRef, ActorValue mapValue, Int placedStage, Int segmentIndex)
    If playerRef == None || mapValue == None || playerRef.GetValue(mapValue) != 1.0
        Return
    EndIf

    If ThisPlayersQuest != None && !ThisPlayersQuest.IsStageDone(placedStage)
        ThisPlayersQuest.SetStage(placedStage)
    EndIf
    playerRef.SetValue(mapValue, 2.0)

    If MapMasterDummyMarker != None
        MapMasterDummyMarker.RevealMapSegment(segmentIndex)
    EndIf
EndFunction
