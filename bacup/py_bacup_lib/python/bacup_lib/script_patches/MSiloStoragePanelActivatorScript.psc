Event OnLoad()
    GoToState("waitingforactivation")
EndEvent

State waitingforactivation
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer()
            Return
        EndIf
        GoToState("processingactivation")
        ResolveStorage().HandlePanelActivation(Self, akActionRef)
        GoToState("waitingforactivation")
    EndEvent
EndState

MSiloQuestScript_Storage Function ResolveStorage()
    If MSiloStorage == None
        Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
        If managerQuest != None && !managerQuest.IsRunning()
            managerQuest.Start()
        EndIf
        MSiloStorage = managerQuest as MSiloQuestScript_Storage
        MSiloStorage.Initialize()
    EndIf
    Return MSiloStorage
EndFunction
