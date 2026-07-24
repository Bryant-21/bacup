Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If akNewLoc == None
        Return
    EndIf

    ObjectReference player = Self.GetReference()
    If player == None
        Return
    EndIf

    If akNewLoc == LocCranberryWatogaLocation
        If PlayerKeywordWatoga.GetReference() != player
            PlayerKeywordWatoga.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocForestWadeAirportLocation
        If PlayerKeywordWadeAirport.GetReference() != player
            PlayerKeywordWadeAirport.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocSwampValleyGalleriaLocation
        If PlayerKeywordValleyGalleria.GetReference() != player
            PlayerKeywordValleyGalleria.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocToxicEasternRegionalPenLocation
        If PlayerKeywordEasternRegional.GetReference() != player
            PlayerKeywordEasternRegional.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocToxicWavyWillardsWaterparkLocation
        If PlayerKeywordWavyWillards.GetReference() != player
            PlayerKeywordWavyWillards.ForceRefTo(player)
        EndIf
    EndIf
EndEvent
