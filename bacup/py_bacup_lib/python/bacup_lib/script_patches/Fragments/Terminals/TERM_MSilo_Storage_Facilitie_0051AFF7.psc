Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    If managerQuest != None && !managerQuest.IsRunning()
        managerQuest.Start()
    EndIf
    (managerQuest as MSiloQuestScript_Storage).FinishMainframeBoot(akTerminalRef)
EndFunction
