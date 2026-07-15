Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    ClueEnableMarker01 = GetLinkedRef(W05_RE_ClueEnableMarker01_Keyword)
    ClueEnableMarker02 = GetLinkedRef(W05_RE_ClueEnableMarker02_Keyword)
    ClueEnableMarker03 = GetLinkedRef(W05_RE_ClueEnableMarker03_Keyword)
    ClueEnableMarker04 = GetLinkedRef(W05_RE_ClueEnableMarker04_Keyword)
    ClueEnableMarker05 = GetLinkedRef(W05_RE_ClueEnableMarker05_Keyword)

    EnableClueMarker(ClueEnableMarker01, activatingPlayer, W05_Clue1_ActorValue)
    EnableClueMarker(ClueEnableMarker02, activatingPlayer, W05_Clue2_ActorValue)
    EnableClueMarker(ClueEnableMarker03, activatingPlayer, W05_Clue3_ActorValue)
    EnableClueMarker(ClueEnableMarker04, activatingPlayer, W05_Clue4_ActorValue)
    EnableClueMarker(ClueEnableMarker05, activatingPlayer, W05_Clue5_ActorValue)
EndEvent

Function EnableClueMarker(ObjectReference clueMarker, Actor playerRef, ActorValue clueValue)
    If clueMarker != None && clueValue != None && playerRef.GetValue(clueValue) >= 1.0
        clueMarker.Enable()
    EndIf
EndFunction
