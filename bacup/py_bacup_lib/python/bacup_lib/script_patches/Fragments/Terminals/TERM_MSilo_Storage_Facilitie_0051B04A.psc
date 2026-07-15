Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ObjectReference[] linkedRefs = akTerminalRef.GetLinkedRefChain(LinkTerminalSwitchDoor, 100)
    Int i = 0
    While i < linkedRefs.Length
        Default2StateActivator linkedSwitch = linkedRefs[i] as Default2StateActivator
        If linkedSwitch != None
            linkedSwitch.IsAnimating = True
            linkedSwitch.SetOpenNoWait(True)
        EndIf
        i = i + 1
    EndWhile
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    If managerQuest != None && !managerQuest.IsRunning()
        managerQuest.Start()
    EndIf
    (managerQuest as MSiloQuestScript_Storage).OpenSecurityDoor()
EndFunction
