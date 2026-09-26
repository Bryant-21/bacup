; FO4's Default2StateActivator is ready in its auto state "waiting"; the FO76
; child's own auto state "Initial" would otherwise leave activation dead.
Function InitializeDestructibleState()
    If ShouldStartDestroyed && !IsDestroyed()
        DamageObject(100000.0)
    EndIf
    If IsDestroyed()
        EnterDestroyedState()
    Else
        GoToState("waiting")
    EndIf
EndFunction

Function EnterDestroyedState()
    String currentState = GetState()
    If currentState == "destroyed" || currentState == "startsdestroyed"
        Return
    EndIf
    repairToState = myState
    GoToState("destroyed")
EndFunction

Function RestoreAfterRepair()
    If IsDestroyed()
        Return
    EndIf
    Bool shouldOpen = repairToState == 0
    If ShouldRepairToFixedOpenState == 1
        shouldOpen = True
    ElseIf ShouldRepairToFixedOpenState == 3
        shouldOpen = False
    EndIf
    GoToState("waiting")
    If isOpen == shouldOpen
        SetDefaultState()
    Else
        SetOpen(shouldOpen)
    EndIf
EndFunction

Function RepairOnActivate(ObjectReference akActionRef)
    If DefaultAliasOnObjectRepaired.TryRepairObject(Self, akActionRef as Actor)
        String currentState = GetState()
        If currentState == "destroyed" || currentState == "startsdestroyed"
            RestoreAfterRepair()
        EndIf
    EndIf
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If IsDestroyed()
        EnterDestroyedState()
    EndIf
EndEvent

Auto State Initial
    Event OnInit()
        InitializeDestructibleState()
    EndEvent

    Event OnLoad()
        InitializeDestructibleState()
        If GetState() == "waiting"
            parent.OnLoad()
        EndIf
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
        InitializeDestructibleState()
        OnActivate(akActionRef)
    EndEvent
EndState

State destroyed
    Event OnActivate(ObjectReference akActionRef)
        RepairOnActivate(akActionRef)
    EndEvent

    Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
        If aiCurrentStage == 0 && !IsDestroyed()
            RestoreAfterRepair()
        EndIf
    EndEvent
EndState

State startsdestroyed
    Event OnActivate(ObjectReference akActionRef)
        RepairOnActivate(akActionRef)
    EndEvent

    Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
        If aiCurrentStage == 0 && !IsDestroyed()
            RestoreAfterRepair()
        EndIf
    EndEvent
EndState
