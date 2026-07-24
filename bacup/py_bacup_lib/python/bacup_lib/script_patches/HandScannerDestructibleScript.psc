State resetredtoblue
    Function SetNextState()
        If !Self.IsDestroyed()
            parent.SetNextState()
        EndIf
    EndFunction
EndState

State Initial
    Event OnInit()
        If shouldStartDestroyed
            Self.SetDestroyed(true)
            Self.GoToState("startsdestroyed")
        Else
            parent.OnInit()
        EndIf
    EndEvent
EndState

State startsdestroyed
    Function SetNextState()
        ; intentional no-op: blocks the inherited SetNextState() fallback from
        ; pulling a destroyed scanner back into a live color state
    EndFunction
EndState

State scanbluetogreen
    Function SetNextState()
        If !Self.IsDestroyed()
            parent.SetNextState()
        EndIf
    EndFunction
EndState

State destroyed
    Function SetNextState()
        ; intentional no-op: blocks the inherited SetNextState() fallback from
        ; pulling a destroyed scanner back into a live color state
    EndFunction
EndState

State resetgreentoblue
    Function SetNextState()
        If !Self.IsDestroyed()
            parent.SetNextState()
        EndIf
    EndFunction
EndState

State scanbluetored
    Function SetNextState()
        If !Self.IsDestroyed()
            parent.SetNextState()
        EndIf
    EndFunction
EndState

Event OnDestructionStageChanged(int aiOldStage, int aiCurrentStage)
    If aiCurrentStage > 0
        If Self.GetState() != "destroyed" && Self.GetState() != "startsdestroyed"
            Self.GoToState("destroyed")
        EndIf
    ElseIf aiCurrentStage == 0 && aiOldStage > 0
        parent.SetNextState()
    EndIf
EndEvent
