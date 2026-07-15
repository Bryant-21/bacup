Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnLocationChange")
    StartSiloQuests()
    HandleLocation(Game.GetPlayer().GetCurrentLocation())
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == Game.GetPlayer()
        HandleLocation(akNewLoc)
    EndIf
EndEvent

Function StartSiloQuests()
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If managerQuest != None && !managerQuest.IsRunning()
        managerQuest.Start()
    EndIf
    If personalQuest != None && !personalQuest.IsRunning()
        personalQuest.Start()
    EndIf
EndFunction

Function HandleLocation(Location akLocation)
    If akLocation == None
        Return
    EndIf

    Int i = 0
    While i < MSiloLocations.Length
        Location siloLocation = MSiloLocations[i]
        If siloLocation == akLocation || siloLocation.IsChild(akLocation)
            StartSiloQuests()
            Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
            Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
            (managerQuest as MSiloQuestScript_Main).SelectLocation(siloLocation)
            (personalQuest as MSiloPersonalQuestScript).BeginSilo(siloLocation)
            Return
        EndIf
        i += 1
    EndWhile
EndFunction
