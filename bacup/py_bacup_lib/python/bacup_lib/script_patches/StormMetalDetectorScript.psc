Event OnInit()
    cooldownTimerID = 1
EndEvent

Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && !cooldownActive
        cooldownActive = True
        If soundMarkerToEnable != None
            soundMarkerToEnable.Enable()
        EndIf
        StartTimer(numberOfSecondsCooldown as Float, cooldownTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == cooldownTimerID
        cooldownActive = False
        If soundMarkerToEnable != None
            soundMarkerToEnable.Disable()
        EndIf
    EndIf
EndEvent
