Event OnQuestInit()
    Quest owner = Self as Quest
    EventQuestScript = owner as DefaultEventQuest
    FillLocationAliases()
    ; FO4 has no RefillAlias, so AliasesToRefill cannot be re-resolved against the chosen location.
    If EventRootAlias != None && EventRootAlias.GetReference() != None && EventQuestScript != None
        EventQuestScript.CenterMarker = EventRootAlias
    EndIf
    If StageToSet >= 0 && !IsStageDone(StageToSet)
        SetStage(StageToSet)
    EndIf
EndEvent

Function FillLocationAliases()
    If LocationList == None || LocationsToFill == None
        Return
    EndIf
    Location[] chosen = new Location[0]
    Int index = 0
    While index < LocationsToFill.Length
        LocationAlias target = LocationsToFill[index]
        If target != None && target.GetLocation() == None
            Location picked = TakeUnusedLocation(chosen)
            If picked == None
                RefillUnusedLocations()
                picked = TakeUnusedLocation(chosen)
            EndIf
            If picked != None
                target.ForceLocationTo(picked)
                chosen.Add(picked)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

; UnusedLocations is a shuffle bag, so repeated runs cycle through the whole list before repeating.
Function RefillUnusedLocations()
    UnusedLocations = new Location[0]
    Int index = 0
    Int size = LocationList.GetSize()
    While index < size
        Location candidate = LocationList.GetAt(index) as Location
        If candidate != None && UnusedLocations.Find(candidate) < 0
            UnusedLocations.Add(candidate)
        EndIf
        index += 1
    EndWhile
EndFunction

Location Function TakeUnusedLocation(Location[] akExcluded)
    If UnusedLocations == None
        Return None
    EndIf
    Int[] candidates = new Int[0]
    Int index = 0
    While index < UnusedLocations.Length
        If UnusedLocations[index] != None && akExcluded.Find(UnusedLocations[index]) < 0
            candidates.Add(index)
        EndIf
        index += 1
    EndWhile
    If candidates.Length == 0
        Return None
    EndIf
    Int slot = candidates[Utility.RandomInt(0, candidates.Length - 1)]
    Location picked = UnusedLocations[slot]
    UnusedLocations.Remove(slot)
    Return picked
EndFunction
