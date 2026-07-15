; FO4 TrapBase has no FO76 Trace wrapper. Keep the child Bool contract through
; the supported user-log API.

Function SetTrapMainHitEnabled(Bool shouldHit)
    PhysicalTrapHit hitScript = Self as PhysicalTrapHit
    If hitScript != None
        hitScript.SetCanHit(shouldHit)
    EndIf
EndFunction

Function ClientFireTrap()
    If !IsPhysicalTrap
        Return
    EndIf

    String fireAnimation = FireTrapAnim
    String fireEvent = FireTrapAnimEndEvent
    If fireAnimation == ""
        fireAnimation = "Trip"
    EndIf
    If fireEvent == ""
        fireEvent = "TransitionComplete"
    EndIf

    SetTrapMainHitEnabled(True)
    PlayAnimationAndWait(fireAnimation, fireEvent)
    SetTrapMainHitEnabled(False)

    If SelfDamageOnFire > 0.0
        DamageObject(SelfDamageOnFire)
    EndIf

    If IsPowered()
        GoToState("fired")
    Else
        GoToState("Idle")
    EndIf
EndFunction

Event OnLoad()
    PrintState()
    SetTrapMainHitEnabled(False)
EndEvent

Event OnCellAttach()
    If GetState() == "Idle" && IsPhysicalTrap && IsPowered()
        GoToState("firing")
    EndIf
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If IsDestroyed()
        CancelTimer(ReFireTimerID)
        SetTrapMainHitEnabled(False)
        GoToState("disarmed")
    EndIf
EndEvent

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity)
    Debug.OpenUserLog("Traps")
    Return Debug.TraceUser("Traps", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction

State Idle
    Event OnBeginState(String asOldState)
        SetTrapMainHitEnabled(False)
        If HasReturnToIdleAnim
            String resetAnimation = ReturnToIdleAnim
            If resetAnimation == ""
                resetAnimation = "Set"
            EndIf
            PlayAnimation(resetAnimation)
        EndIf
    EndEvent

    Event OnPowerOn(ObjectReference akPowerGenerator)
        If IsPhysicalTrap && !isGrabbed
            GoToState("firing")
        EndIf
    EndEvent

    Event OnActivate(ObjectReference akActivator)
        If IsPhysicalTrap
            If akActivator as Actor
                parent.OnActivate(akActivator)
            Else
                GoToState("firing")
            EndIf
        Else
            parent.OnActivate(akActivator)
        EndIf
    EndEvent
EndState

State fired
    Event OnBeginState(String asOldState)
        If IsPhysicalTrap && ReFireTime > 0.0 && IsPowered()
            StartTimer(ReFireTime, ReFireTimerID)
        EndIf
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == ReFireTimerID && IsPowered()
            GoToState("firing")
        EndIf
    EndEvent

    Event OnPowerOff()
        CancelTimer(ReFireTimerID)
        SetTrapMainHitEnabled(False)
        GoToState("Idle")
    EndEvent
EndState

State disarmed
    Event OnBeginState(String asOldState)
        SetTrapMainHitEnabled(False)
        PlayAnimation(DisarmAnim)
    EndEvent

    Event OnLoad()
        PlayAnimation(ReturnToIdleAnim)
        PlayAnimation(DisarmAnim)
    EndEvent
EndState
