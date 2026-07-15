; The base activator exposes a local Trigger prompt. Keep activation confined to
; the armed state and use the client PEX's existing linked-effect cycle.

Function Trip()
    If SparkEffectTime <= 0.0
        SparkEffectTime = 1.0
    EndIf
    GoToState("On")
EndFunction

State On
    Event OnBeginState(String asOldState)
        ObjectReference sparkEffect = GetLinkedRef(LinkCustom01)
        If sparkEffect == None
            GoToState("Off")
            Return
        EndIf
        sparkEffect.EnableNoWait(False)
        StartTimer(SparkEffectTime, EffectTimerID)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID == EffectTimerID
            ObjectReference sparkEffect = GetLinkedRef(LinkCustom01)
            If sparkEffect != None
                sparkEffect.DisableNoWait(False)
            EndIf
            GoToState("Off")
        EndIf
    EndEvent
EndState

State Off
    Event OnActivate(ObjectReference akActivator)
        Trip()
    EndEvent
EndState
