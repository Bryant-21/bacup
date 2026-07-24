Event OnActivate(ObjectReference akActionRef)
    If lock_SecretDoor || openState != CONST_OpenState_CLOSED
        Return
    EndIf

    Actor actionActor = akActionRef as Actor
    If actionActor == None || !actionActor.WornHasKeyword(MoMVeilItemKeyword) || (MoMMaster != None && !MoMMaster.IsRunning())
        Return
    EndIf

    lock_SecretDoor = True

    If OpenDelay > 0
        StartTimer(OpenDelay, 1)
    Else
        OpenSecretDoor()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        OpenSecretDoor()
    ElseIf aiTimerID == 2
        ObjectReference linkedDoor = GetLinkedRef()
        If linkedDoor != None
            linkedDoor.SetOpen(False)
        EndIf
        openState = CONST_OpenState_CLOSED
        isSecretDoorTimerRunning = False
    EndIf
EndEvent

Function OpenSecretDoor()
    ObjectReference linkedDoor = GetLinkedRef()
    If linkedDoor != None
        linkedDoor.SetOpen(True)
    EndIf
    openState = CONST_OpenState_OPEN
    lock_SecretDoor = False

    If CloseDelay > 0
        StartTimer(CloseDelay, 2)
        isSecretDoorTimerRunning = True
        openState = CONST_OpenState_CLOSING
    EndIf
EndFunction
