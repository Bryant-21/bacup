Function KillJohnnyInSecurityRoom()
    If Johnny == None
        Return
    EndIf

    Actor johnnyRef = Johnny.GetActorReference()
    If johnnyRef == None || johnnyRef.IsDead()
        Return
    EndIf
    StartTimer(JohnnyDeathTimerLength, JohnnyDeathTimerId)
EndFunction

Event OnTimer(int aiTimerID)
    If aiTimerID != JohnnyDeathTimerId || Johnny == None || Health == None
        Return
    EndIf

    Actor johnnyRef = Johnny.GetActorReference()
    If johnnyRef == None || johnnyRef.IsDead()
        Return
    EndIf
    johnnyRef.DamageValue(Health, HealthDamageAmount)
EndEvent

Function PrepareFacilitiesManagement()
    ; The keycard alias is created in the holding cell; FO76's stripped quest script staged it on the Facilities Management marker.
    ObjectReference keycardRef = FacilitiesManagementKeycard.GetReference()
    ObjectReference markerRef = FacilitiesManagementKeycardMarker.GetReference()
    If keycardRef != None && markerRef != None && keycardRef.GetContainer() == None && keycardRef.GetParentCell() != markerRef.GetParentCell()
        keycardRef.MoveTo(markerRef)
        keycardRef.Enable()
    EndIf

    ; The converted breaker base lost FO76's CircuitBreakerScript, so the quest listens for the flip itself.
    ObjectReference breakerRef = Breaker.GetReference()
    If breakerRef != None && !IsStageDone(JohnnySecurityStage)
        RegisterForRemoteEvent(breakerRef, "OnActivate")
    EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akSender != Breaker.GetReference() || akActionRef != Game.GetPlayer()
        Return
    EndIf
    UnregisterForRemoteEvent(akSender, "OnActivate")
    If IsStageDone(JohnnySecurityStage)
        Return
    EndIf

    ObjectReference exitDoorRef = DecontaminationExitDoor.GetReference()
    If exitDoorRef != None
        exitDoorRef.Lock(False)
        exitDoorRef.SetOpen(True)
    EndIf
    SetStage(JohnnySecurityStage)
EndEvent
