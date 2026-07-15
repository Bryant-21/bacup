Function ClearSpawnedEffects()
    CancelTimer(WarnTimerID)
    CancelTimer(HazardTimerId)

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
    If TripExplosion != None
        TripExplosion.Delete()
        TripExplosion = None
    EndIf
EndFunction

Function TryStartTrap(ObjectReference akActionRef)
    Actor actionActor = akActionRef as Actor
    If actionActor != None && (RangeToTrip <= 0.0 || GetDistance(actionActor) <= RangeToTrip)
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
        If ExplosionToSpawn != None
            TripExplosion = PlaceAtMe(ExplosionToSpawn)
        EndIf
        If VFX != None
            TrapVFX = PlaceAtMe(VFX as Form)
        EndIf
        If FloraHazard01 != None
            TrapHazard01 = PlaceAtMe(FloraHazard01 as Form)
        EndIf
        If FloraHazard02 != None
            TrapHazard02 = PlaceAtMe(FloraHazard02 as Form)
        EndIf
        StartTimer(HazardTime, HazardTimerId)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == HazardTimerId
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
    EndEvent
EndState

State Waiting
    Event OnBeginState(String asOldState)
        ClearSpawnedEffects()
        TriggeredActor = None
    EndEvent

    Event OnTriggerEnter(ObjectReference akActionRef)
        TryStartTrap(akActionRef)
    EndEvent

    Event OnActivate(ObjectReference akActionRef)
        TryStartTrap(akActionRef)
    EndEvent
EndState
