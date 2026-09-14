Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnLocationChange")
    If !HandleLocation(Game.GetPlayer().GetCurrentLocation())
        StartTimer(0.5, 7301)
    EndIf
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer()
        If !HandleLocation(akNewLoc)
            StartTimer(0.5, 7301)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7301
        HandleLocation(Game.GetPlayer().GetCurrentLocation())
    EndIf
EndEvent

Bool Function StartSiloQuests(Location akLocation)
    StartPreparedSilo(akLocation)
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If managerQuest == None || personalQuest == None
        Return False
    EndIf
    Return (managerQuest.IsRunning() || managerQuest.IsCompleted()) && (personalQuest.IsRunning() || personalQuest.IsCompleted())
EndFunction

Bool Function StartPreparedSilo(Location akLocation, ObjectReference akEntryRef = None)
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    MSiloPersonalQuestScript personal = personalQuest as MSiloPersonalQuestScript
    Actor player = Game.GetPlayer()
    Keyword personalQuestKeyword = Game.GetFormFromFile(0x003E03AB, "SeventySix.esm") as Keyword
    If managerQuest == None || personalQuest == None || personal == None || player == None || akLocation == None || MSiloQuestKeyword == None || personalQuestKeyword == None
        Return False
    EndIf

    ObjectReference eventRef = akEntryRef
    If eventRef == None
        eventRef = player
    EndIf
    If !managerQuest.IsRunning() && !managerQuest.IsCompleted()
        MSiloQuestKeyword.SendStoryEventAndWait(akLocation, player, eventRef, 0, 0)
    EndIf
    If !managerQuest.IsRunning() && !managerQuest.IsCompleted()
        Return False
    EndIf

    If !personal.PrepareSiloAliases(akLocation)
        Return False
    EndIf
    Location eventLocation = akLocation
    LocationAlias managerLocationAlias = managerQuest.GetAlias(2) as LocationAlias
    If managerLocationAlias != None && managerLocationAlias.GetLocation() != None
        eventLocation = managerLocationAlias.GetLocation()
    EndIf
    If !personalQuest.IsRunning() && !personalQuest.IsCompleted()
        personalQuestKeyword.SendStoryEventAndWait(eventLocation, player, eventRef, 0, 0)
    EndIf
    Return (managerQuest.IsRunning() || managerQuest.IsCompleted()) && (personalQuest.IsRunning() || personalQuest.IsCompleted())
EndFunction

Bool Function HandleLocation(Location akLocation)
    If akLocation == None
        Return False
    EndIf

    Int i = 0
    While i < MSiloLocations.Length
        Location siloLocation = MSiloLocations[i]
        If siloLocation == akLocation || siloLocation.IsChild(akLocation)
            If !StartSiloQuests(siloLocation)
                Return False
            EndIf
            Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
            (personalQuest as MSiloPersonalQuestScript).BeginSilo(siloLocation)
            Return True
        EndIf
        i += 1
    EndWhile
    Return True
EndFunction
