Function ClearSpawnedEffects()
    CancelTimer(WarnTimerID)
    CancelTimer(HazardTimerId)
    CancelTimer(RemoteTriggerTimerID)

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

Function TriggerNearbyFirecrackerBerries()
    Float searchRadius = NearbyFirecrackerBerriesRadius
    If searchRadius <= 0.0
        searchRadius = RangeToTrip
    EndIf
    If FireCrackerBerryFormList == None || searchRadius <= 0.0
        Return
    EndIf

    ObjectReference[] nearbyFirecrackerBerries = FindAllReferencesOfType(FireCrackerBerryFormList as Form, searchRadius)
    Int count = 0
    While count < nearbyFirecrackerBerries.Length
        TrapFloraFirecrackerScript firecrackerBerry = nearbyFirecrackerBerries[count] as TrapFloraFirecrackerScript
        If firecrackerBerry != None && firecrackerBerry != Self && firecrackerBerry.GetState() == "Waiting"
            firecrackerBerry.StartTimer(Utility.RandomFloat(0.0, 0.2), RemoteTriggerTimerID)
        EndIf
        count += 1
    EndWhile
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
        PlayAnimation("Play03")
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
        PlayAnimation("Play01")
        If VFX != None
            TrapVFX = PlaceAtMe(VFX as Form)
        EndIf
        If FloraHazard01 != None
            TrapHazard01 = PlaceAtMe(FloraHazard01 as Form)
        EndIf
        If FloraHazard02 != None
            TrapHazard02 = PlaceAtMe(FloraHazard02 as Form)
        EndIf
        If FireCrackerBerryFormList != None && (NearbyFirecrackerBerriesRadius > 0.0 || RangeToTrip > 0.0)
            TriggerNearbyFirecrackerBerries()
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
        PlayAnimation("JumpState02")
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

    Event OnTimer(Int aiTimerID)
        If aiTimerID == RemoteTriggerTimerID
            GoToState("warning")
        EndIf
    EndEvent
EndState
