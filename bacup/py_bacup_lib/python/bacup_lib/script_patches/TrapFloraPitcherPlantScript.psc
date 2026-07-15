Function ClearSpawnedEffects()
    CancelTimer(WarnTimerID)
    CancelTimer(HazardTimerId)
    CancelTimer(LidOpenTimeId)

    If TrapHazard01 != None
        TrapHazard01.Delete()
        TrapHazard01 = None
    EndIf
    If TrapHazard02 != None
        TrapHazard02.Delete()
        TrapHazard02 = None
    EndIf
    If TrapVFX != None
        TrapVFX.Delete()
        TrapVFX = None
    EndIf
EndFunction

Function TryStartTrap(ObjectReference akActionRef)
    Actor actionActor = akActionRef as Actor
    If actionActor != None
        TriggeredActor = actionActor
        GoToState("warning")
    EndIf
EndFunction

Event OnReset()
    ClearSpawnedEffects()
    TriggeredActor = None
    GoToState("Waiting")
EndEvent

State warning
    Event OnBeginState(String asOldState)
        PlayAnimation("Play01")
        StartTimer(WarnTime, WarnTimerID)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == WarnTimerID
            GoToState("triggercomplete")
        EndIf
    EndEvent

    Event OnUnload()
        ClearSpawnedEffects()
        GoToState("Waiting")
    EndEvent
EndState

State triggercomplete
    Event OnBeginState(String asOldState)
        PlayAnimation("JumpState02")
        If VFX != None
            TrapVFX = PlaceAtMe(VFX as Form)
        EndIf
        If FloraHazard01 != None
            TrapHazard01 = PlaceAtMe(FloraHazard01 as Form)
        EndIf
        If FloraHazard02 != None
            TrapHazard02 = PlaceAtMe(FloraHazard02 as Form)
        EndIf
        StartTimer(LidOpenTime, LidOpenTimeId)
        StartTimer(HazardTime, HazardTimerId)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == LidOpenTimeId
            PlayAnimation("Play02")
        ElseIf aiTimerID == HazardTimerId
            GoToState("disarmed")
        EndIf
    EndEvent

    Event OnUnload()
        GoToState("disarmed")
    EndEvent
EndState

State disarmed
    Event OnBeginState(String asOldState)
        ClearSpawnedEffects()
        TriggeredActor = None
        PlayAnimation("SoftJump")
    EndEvent
EndState

State Waiting
    Event OnLoad()
        PlayAnimation("JumpState01")
    EndEvent

    Event OnBeginState(String asOldState)
        ClearSpawnedEffects()
        TriggeredActor = None
        PlayAnimation("JumpState01")
    EndEvent

    Event OnTriggerEnter(ObjectReference akActionRef)
        TryStartTrap(akActionRef)
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
        TryStartTrap(akActionRef)
    EndEvent
EndState
