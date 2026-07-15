Event OnLoad()
    EnsureSiloQuests()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        EnsureSiloQuests()
        Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
        (personalQuest as MSiloPersonalQuestScript).BeginSilo(Game.GetPlayer().GetCurrentLocation())
    EndIf
EndEvent

Function EnsureSiloQuests()
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If managerQuest != None && !managerQuest.IsRunning()
        managerQuest.Start()
    EndIf
    If personalQuest != None && !personalQuest.IsRunning()
        personalQuest.Start()
    EndIf
EndFunction
