; Drive the local motor and the co-attached hit processor from FO4 workshop
; power events. The source record leaves TranslationSpeed unbound, so use the
; equivalent FO4 sawblade default when it has no configured value.

Function SetSawbladeHitEnabled(Bool shouldHit)
    PhysicalTrapHit hitScript = Self as PhysicalTrapHit
    If hitScript != None
        hitScript.SetCanHit(shouldHit)
    EndIf
EndFunction

Event OnCellAttach()
    BlockActivation(True)
    If !disarmed
        If IsPowered()
            GoToState("active")
        Else
            GoToState("Idle")
        EndIf
    EndIf
EndEvent

Event OnWorkshopObjectGrabbed(ObjectReference akReference)
    isGrabbed = True
EndEvent

Event OnWorkshopObjectMoved(ObjectReference akReference)
    isGrabbed = False
    If !disarmed
        If IsPowered()
            GoToState("active")
        Else
            GoToState("Idle")
        EndIf
    EndIf
EndEvent

Event OnWorkshopObjectRepaired(ObjectReference akReference)
    ClearDestruction()
    SetDestroyed(False)
    disarmed = False
    If IsPowered()
        GoToState("active")
    Else
        GoToState("Idle")
    EndIf
EndEvent

Event OnReset()
    ClearDestruction()
    SetDestroyed(False)
    disarmed = False
    isGrabbed = False
    If IsPowered()
        GoToState("active")
    Else
        GoToState("Idle")
    EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage > aiOldStage
        DisarmTrap()
    EndIf
EndEvent

Function DisarmTrap()
    If !disarmed
        GoToState("disarm")
    EndIf
EndFunction

State Idle
    Event OnBeginState(String asOldState)
        SetAnimationVariableFloat(movementAnimString, 0.0)
        SetSawbladeHitEnabled(False)
    EndEvent

    Event OnPowerOn(ObjectReference akPowerGenerator)
        If !disarmed && !isGrabbed
            GoToState("active")
        EndIf
    EndEvent
EndState

State active
    Event OnBeginState(String asOldState)
        If !disarmed
            Float motorSpeed = TranslationSpeed
            If motorSpeed <= 0.0
                motorSpeed = 1.0
            EndIf
            SetAnimationVariableFloat(movementAnimString, motorSpeed)
            SetSawbladeHitEnabled(True)
        EndIf
    EndEvent

    Event OnPowerOff()
        GoToState("Idle")
    EndEvent
EndState

State disarm
    Event OnBeginState(String asOldState)
        disarmed = True
        SetAnimationVariableFloat(movementAnimString, 0.0)
        SetSawbladeHitEnabled(False)
        If DisarmSound != None && Is3DLoaded()
            DisarmSound.Play(Self)
        EndIf
        SetDestroyed()
    EndEvent
EndState
