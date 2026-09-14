Event OnLoad()
    EnsureSiloQuests(Game.GetPlayer().GetCurrentLocation())
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        EnsureSiloQuests(Game.GetPlayer().GetCurrentLocation())
        Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
        If personalQuest != None && personalQuest.IsRunning()
            (personalQuest as MSiloPersonalQuestScript).BeginSilo(Game.GetPlayer().GetCurrentLocation())
        EndIf
    EndIf
EndEvent

Bool Function EnsureSiloQuests(Location akLocation)
    Quest startupQuest = Game.GetFormFromFile(0x0050FDEE, "SeventySix.esm") as Quest
    MSiloStartupQuestScript startup = startupQuest as MSiloStartupQuestScript
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If startup == None || managerQuest == None || personalQuest == None || akLocation == None
        Return False
    EndIf
    startup.StartPreparedSilo(akLocation, Self)
    Return (managerQuest.IsRunning() || managerQuest.IsCompleted()) && (personalQuest.IsRunning() || personalQuest.IsCompleted())
EndFunction
