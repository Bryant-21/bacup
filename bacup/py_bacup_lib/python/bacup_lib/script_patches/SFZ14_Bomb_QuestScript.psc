Function PrepareBombs()
    ClearBombTracking()

    If BombsToUse == None || BombsChosen == None
        BombsWanted = 0
        Return
    EndIf

    Int locationSet = Utility.RandomInt(0, 3)
    BombsWanted = Utility.RandomInt(3, 5)
    Int placedBombs = 0
    Int index = 0
    While index < BombsToUse.Length
        If BombsToUse[index].IntAssigned <= BombsWanted
            ReferenceAlias bombAlias = BombsToUse[index].BombAlias
            ReferenceAlias markerAlias = BombsToUse[index].BombMarkerAlias
            ObjectReference bombRef = None
            ObjectReference markerRef = None
            If bombAlias != None
                bombRef = bombAlias.GetReference()
            EndIf
            If markerAlias != None
                markerRef = markerAlias.GetReference()
            EndIf
            If markerRef == None
                markerRef = GetAuthoredBombMarker(locationSet, BombsToUse[index].IntAssigned)
            EndIf
            If bombRef != None && markerRef != None
                bombRef.MoveTo(markerRef)
                bombRef.AddKeyword(SFZ14_Bomb_ChosenBombKeyword)
                BombsChosen.AddRef(bombRef)
                placedBombs += 1
            EndIf
        EndIf
        index += 1
    EndWhile

    BombsWanted = placedBombs
    DefaultAliasInventoryManagement inventoryManager = SFZ14Player as DefaultAliasInventoryManagement
    If inventoryManager != None && BombsWanted >= 3
        inventoryManager.SetRequiredAmount(BombsWanted)
    EndIf
EndFunction

; FO76 filled BombMarker01-05 with "find matching reference, in ChosenBombLocation,
; with LocRefType X". Nothing fills that location alias under FO4 — its only source
; is BombLocations' "force into alias when filled", and BombLocations itself carries
; conditions but no FO4-valid fill type — so every marker alias resolves to None and
; no bomb is ever placed. Resolve the four authored marker sets directly, keeping the
; daily's rotation between them.
ObjectReference Function GetAuthoredBombMarker(Int locationSet, Int slot)
    Int[] markerIDs = new Int[20]
    markerIDs[0] = 0x0015D144   ; SFZ14_Bomb_Galleria01Ref
    markerIDs[1] = 0x0015D14A   ; SFZ14_Bomb_Galleria02Ref
    markerIDs[2] = 0x0015D14B   ; SFZ14_Bomb_Galleria03Ref
    markerIDs[3] = 0x003AF16E   ; SFZ14_Bomb_Galleria04Ref
    markerIDs[4] = 0x003AF16D   ; SFZ14_Bomb_Galleria05Ref
    markerIDs[5] = 0x003B2628   ; SFZ14_Bomb_Dyer01Ref
    markerIDs[6] = 0x003B2629   ; SFZ14_Bomb_Dyer02Ref
    markerIDs[7] = 0x003B262A   ; SFZ14_Bomb_Dyer03Ref
    markerIDs[8] = 0x003B262B   ; SFZ14_Bomb_Dyer04Ref
    markerIDs[9] = 0x003B262C   ; SFZ14_Bomb_Dyer05Ref
    markerIDs[10] = 0x003B262D  ; SFZ14_Bomb_Crevasse01Ref
    markerIDs[11] = 0x003B262E  ; SFZ14_Bomb_Crevasse02Ref
    markerIDs[12] = 0x003B262F  ; SFZ14_Bomb_Crevasse03Ref
    markerIDs[13] = 0x003B2630  ; SFZ14_Bomb_Crevasse04Ref
    markerIDs[14] = 0x003B2631  ; SFZ14_Bomb_Crevasse05Ref
    markerIDs[15] = 0x0000A865  ; SFZ14_Bomb_Factory01Ref
    markerIDs[16] = 0x0000A867  ; SFZ14_Bomb_Factory02Ref
    markerIDs[17] = 0x0000A866  ; SFZ14_Bomb_Factory03Ref
    markerIDs[18] = 0x003AF171  ; SFZ14_Bomb_Factory04Ref
    markerIDs[19] = 0x003AF172  ; SFZ14_Bomb_Factory05Ref

    Int markerIndex = locationSet * 5 + slot - 1
    If markerIndex < 0 || markerIndex >= markerIDs.Length
        Return None
    EndIf
    Return Game.GetFormFromFile(markerIDs[markerIndex], "SeventySix.esm") as ObjectReference
EndFunction

Function ClearBombTracking()
    If BombsChosen == None
        Return
    EndIf

    Int index = BombsChosen.GetCount() - 1
    While index >= 0
        ObjectReference bombRef = BombsChosen.GetAt(index)
        If bombRef != None
            BombsChosen.RemoveRef(bombRef)
            bombRef.ResetKeyword(SFZ14_Bomb_ChosenBombKeyword)
        EndIf
        index -= 1
    EndWhile
EndFunction
