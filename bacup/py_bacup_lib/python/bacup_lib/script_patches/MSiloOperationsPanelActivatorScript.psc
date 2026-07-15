Event OnLoad()
    If GetCurrentDestructionStage() > 0
        GoToState("destroyed")
    Else
        GoToState("intact")
    EndIf
EndEvent

State intact
    Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
        If aiCurrentStage > aiOldStage
            ResolveOperations().HandlePanelDestroyed(Self)
            GoToState("destroyed")
        EndIf
    EndEvent
EndState

MSiloQuestScript_Operations Function ResolveOperations()
    Quest managerQuest = Game.GetFormFromFile(0x003D72E6, "SeventySix.esm") as Quest
    If managerQuest != None && !managerQuest.IsRunning()
        managerQuest.Start()
    EndIf
    MSiloQuestScript_Operations operations = managerQuest as MSiloQuestScript_Operations
    operations.Initialize()
    Return operations
EndFunction
