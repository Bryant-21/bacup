; Restore the stripped trigger-count/state behavior and replace the FO76-only
; TrapBase.Trace wrapper.

Function SetSpikeHitEnabled(Bool shouldHit)
    PhysicalTrapHit hitScript = Self as PhysicalTrapHit
    If hitScript != None
        hitScript.SetCanHit(shouldHit)
    EndIf
EndFunction

Function CountSet(Int NewCount)
    If NewCount < 0
        NewCount = 0
    EndIf
    CountActual = NewCount

    Bool shouldLower = NewCount > 0
    If PlateLoweredActual != shouldLower
        PlateLoweredSet(shouldLower)
    EndIf
EndFunction

Function CheckCount()
    GoToState("busy")
    Count = GetTriggerObjectCount()
    If !IsDestroyed()
        GoToState("Ready")
    EndIf
EndFunction

Function PlateLoweredSet(Bool SetLowered)
    If PlateLoweredActual == SetLowered
        Return
    EndIf

    PlateLoweredActual = SetLowered
    If SetLowered
        GoToState("busy")
        SetSpikeHitEnabled(True)

        String lowerAnimation = TriggerAnim
        String lowerEvent = TriggerEvent
        If lowerAnimation == ""
            lowerAnimation = "Trip"
        EndIf
        If lowerEvent == ""
            lowerEvent = "TransitionComplete"
        EndIf

        If WaitForAnimEnd || NoHitAfterAnim
            PlayAnimationAndWait(lowerAnimation, lowerEvent)
            If NoHitAfterAnim
                SetSpikeHitEnabled(False)
            EndIf
        Else
            PlayAnimation(lowerAnimation)
        EndIf
    Else
        SetSpikeHitEnabled(False)

        String raiseAnimation = ResetAnim
        String raiseEvent = ResetEvent
        If raiseAnimation == ""
            raiseAnimation = "Set"
        EndIf
        If raiseEvent == ""
            raiseEvent = "TransitionComplete"
        EndIf

        If WaitForAnimEnd
            PlayAnimationAndWait(raiseAnimation, raiseEvent)
        Else
            PlayAnimation(raiseAnimation)
        EndIf
        GoToState("Ready")
    EndIf
    CheckCount()
EndFunction

Event OnWorkshopObjectRepaired(ObjectReference akReference)
    ClearDestruction()
    SetDestroyed(False)
    SetSpikeHitEnabled(False)
    CountActual = 0
    PlateLoweredActual = False
    String repairAnimation = RepairAnim
    If repairAnimation == ""
        repairAnimation = "Set"
    EndIf
    PlayAnimation(repairAnimation)
    GoToState("Ready")
EndEvent

Event OnReset()
    ClearDestruction()
    SetDestroyed(False)
    SetSpikeHitEnabled(False)
    CountActual = 0
    PlateLoweredActual = False
    String resetAnimation = ResetAnim
    If resetAnimation == ""
        resetAnimation = "Set"
    EndIf
    PlayAnimation(resetAnimation)
    GoToState("Ready")
    CheckCount()
EndEvent

Event OnCellAttach()
    BlockActivation(True)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage > aiOldStage && IsDestroyed()
        SetSpikeHitEnabled(False)
        GoToState("broken")
    EndIf
EndEvent

State broken
    Event OnBeginState(String asOldState)
        SetSpikeHitEnabled(False)
        PlayAnimation(DestroyedAnim)
    EndEvent
EndState

State Ready
    Event OnBeginState(String asOldState)
        If asOldState == "broken"
            CheckCount()
        EndIf
    EndEvent

    Event OnTriggerEnter(ObjectReference akActionRef)
        CheckCount()
    EndEvent

    Event OnTriggerLeave(ObjectReference akActionRef)
        CheckCount()
    EndEvent

    Event OnCellLoad()
        CheckCount()
    EndEvent

    Event OnActivate(ObjectReference akActivator)
        If akActivator as Actor && !PlateLoweredActual
            PlateLoweredSet(True)
            Utility.Wait(0.3)
            CheckCount()
        EndIf
    EndEvent
EndState

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity)
    Debug.OpenUserLog("Traps")
    Return Debug.TraceUser("Traps", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
