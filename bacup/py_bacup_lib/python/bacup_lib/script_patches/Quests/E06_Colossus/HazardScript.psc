Float Function RandomSeconds(Float afMinimum, Float afMaximum)
    If afMinimum < 0.5
        afMinimum = 0.5
    EndIf
    If afMaximum < afMinimum
        afMaximum = afMinimum
    EndIf
    Return Utility.RandomFloat(afMinimum, afMaximum)
EndFunction

Default2StateActivator Function HazardActivator()
    ObjectReference selfRef = Self as ObjectReference
    Return selfRef as Default2StateActivator
EndFunction

; The TwoStateGraphLoop model shows the rumble/dust warning while closed and the falling embers while open.
Function SetHazardOpen(Bool abOpen)
    Default2StateActivator twoState = HazardActivator()
    If twoState == None || twoState.isOpen == abOpen
        Return
    EndIf
    If Is3DLoaded()
        twoState.AllowInterrupt = True
        twoState.SetOpenNoWait(abOpen)
    Else
        twoState.isOpen = abOpen
    EndIf
EndFunction

Bool Function EncounterRunning()
    Return E06_Colossus != None && E06_Colossus.IsRunning()
EndFunction

Bool Function IsHazardActive()
    Return False
EndFunction

Function StartHazard()
    If EncounterRunning()
        GoToState("hazardwarning")
    EndIf
EndFunction

Function StopHazard()
    CancelTimer(hazardStateTimerID)
    CancelTimer(durationTimerID)
    SetHazardOpen(False)
    If !IsDisabled()
        DisableNoWait()
    EndIf
EndFunction

State hazardoff
    Event OnBeginState(String asOldState)
        CancelTimer(hazardStateTimerID)
        CancelTimer(durationTimerID)
        Default2StateActivator twoState = HazardActivator()
        If twoState != None && twoState.isOpen && Is3DLoaded() && !IsDisabled()
            ; Let the closing animation clear the embers before the ref disappears.
            SetHazardOpen(False)
            StartTimer(7.0, hazardStateTimerID)
        Else
            StopHazard()
        EndIf
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == hazardStateTimerID
            StopHazard()
        EndIf
    EndEvent
EndState

State hazardwarning
    Event OnBeginState(String asOldState)
        CancelTimer(durationTimerID)
        SetHazardOpen(False)
        If IsDisabled()
            EnableNoWait()
        EndIf
        StartTimer(RandomSeconds(MinStateChange, MaxStateChange), hazardStateTimerID)
    EndEvent

    Bool Function IsHazardActive()
        Return True
    EndFunction

    Function StartHazard()
    EndFunction

    Function StopHazard()
        GoToState("hazardoff")
    EndFunction

    Event OnTimer(Int aiTimerID)
        If aiTimerID != hazardStateTimerID
            Return
        EndIf
        If EncounterRunning()
            GoToState("hazardon")
        Else
            GoToState("hazardoff")
        EndIf
    EndEvent
EndState

State hazardon
    Event OnBeginState(String asOldState)
        CancelTimer(hazardStateTimerID)
        If IsDisabled()
            EnableNoWait()
        EndIf
        SetHazardOpen(True)
        StartTimer(RandomSeconds(MinDuration, MaxDuration), durationTimerID)
    EndEvent

    Bool Function IsHazardActive()
        Return True
    EndFunction

    Function StartHazard()
    EndFunction

    Function StopHazard()
        GoToState("hazardoff")
    EndFunction

    Event OnTimer(Int aiTimerID)
        If aiTimerID == durationTimerID
            GoToState("hazardoff")
        EndIf
    EndEvent
EndState
