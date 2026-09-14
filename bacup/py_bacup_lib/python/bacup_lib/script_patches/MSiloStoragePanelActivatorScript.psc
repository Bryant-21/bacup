Event OnLoad()
    GoToState("waitingforactivation")
EndEvent

State waitingforactivation
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer()
            Return
        EndIf
        GoToState("processingactivation")
        MSiloQuestScript_Storage storage = ResolveStorage()
        If storage != None
            storage.HandlePanelActivation(Self, akActionRef)
        EndIf
        GoToState("waitingforactivation")
    EndEvent
EndState

MSiloQuestScript_Storage Function ResolveStorage()
    If MSiloStorage == None
        Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
        If managerQuest == None || !managerQuest.IsRunning()
            Return None
        EndIf
        MSiloStorage = managerQuest as MSiloQuestScript_Storage
        MSiloStorage.Initialize()
    EndIf
    Return MSiloStorage
EndFunction
