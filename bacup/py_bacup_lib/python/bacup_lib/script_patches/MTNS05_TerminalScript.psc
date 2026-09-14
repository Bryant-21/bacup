Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.GetStage() >= TerminalAccessedStage
        GoToState("terminalaccessed")
    Else
        GoToState("init")
    EndIf
EndEvent

State init
    Event OnActivate(ObjectReference akActivator)
        If akActivator != Game.GetPlayer()
            Return
        EndIf

        Quest owningQuest = GetOwningQuest()
        If owningQuest != None && owningQuest.GetStage() < TerminalAccessedStage
            owningQuest.SetStage(TerminalAccessedStage)
        EndIf
        GoToState("terminalaccessed")
    EndEvent
EndState

State terminalaccessed
    Event OnActivate(ObjectReference akActivator)
    EndEvent
EndState
