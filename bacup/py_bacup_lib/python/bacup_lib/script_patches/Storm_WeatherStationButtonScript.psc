Function ApplyRemoteState(Bool openState)
    bIsOpen = openState
    If Is3DLoaded()
        If bIsOpen
            PlayAnimation(DoorOpenAnim)
        Else
            PlayAnimation(DoorClosedAnim)
        EndIf
    EndIf
EndFunction

Function ApplyLocalState(Bool openState)
    ApplyRemoteState(openState)
    Storm_WeatherStationButtonScript linkedButton = GetLinkedRef(LinkedButtonKeyword) as Storm_WeatherStationButtonScript
    If linkedButton != None && linkedButton != Self
        linkedButton.ApplyRemoteState(openState)
    EndIf
EndFunction

Function UpdateLinkedButtonState(Actor akPlayer)
    ObjectReference[] linkedRefs = GetLinkedRefChain(LinkedRefToActivate)
    If linkedRefs != None
        Int i = 0
        While i < linkedRefs.Length
            If linkedRefs[i] != None
                linkedRefs[i].Activate(akPlayer)
            EndIf
            i += 1
        EndWhile
    EndIf
    ApplyLocalState(bIsOpen)
EndFunction

; OnSyncVariableNetworkChanged("bIsOpen") was the FO76 server->client replication
; callback. bIsOpen is now only written by OnActivate/OnTimer here, and both already
; call UpdateLinkedButtonState -> ApplyLocalState, so the handler is redundant.
; @drop-member OnSyncVariableNetworkChanged

Event OnActivate(ObjectReference akActionRef)
    If bIsOpen || !IsEnabled()
        Return
    EndIf

    Actor activatingActor = akActionRef as Actor
    If activatingActor == None
        Return
    EndIf
    If Is3DLoaded()
        If PressAnimEventName != ""
            PlayAnimationAndWait(PressAnim, PressAnimEventName)
        Else
            PlayAnimation(PressAnim)
        EndIf
    EndIf

    bIsOpen = True
    UpdateLinkedButtonState(activatingActor)
    If AutoCloseDelay > 0.0
        StartTimer(AutoCloseDelay, iAutoCloseTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iAutoCloseTimerID && bIsOpen
        bIsOpen = False
        UpdateLinkedButtonState(Game.GetPlayer())
    EndIf
EndEvent

Event OnLoad()
    ApplyRemoteState(bIsOpen)
EndEvent
