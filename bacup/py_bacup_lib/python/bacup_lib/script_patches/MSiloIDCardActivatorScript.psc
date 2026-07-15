Event OnLoad()
    GoToState("waitingforactivation")
EndEvent

State waitingforactivation
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer()
            Return
        EndIf
        GoToState("processingactivation")
        ResolveResidential().HandleIDCardActivation(Self, akActionRef)
        GoToState("waitingforactivation")
    EndEvent
EndState

MSiloQuestScript_Residential Function ResolveResidential()
    If MSiloResidential == None
        Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
        If managerQuest != None && !managerQuest.IsRunning()
            managerQuest.Start()
        EndIf
        MSiloResidential = managerQuest as MSiloQuestScript_Residential
        MSiloResidential.Initialize()
    EndIf
    Return MSiloResidential
EndFunction
